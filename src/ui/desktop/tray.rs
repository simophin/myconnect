//! The tray icon and its menu. The shell decides what the menu holds
//! ([`TrayItem`]s carrying [`TrayCommand`]s); the tray shows it and reports
//! what was chosen as a [`DesktopEvent`](super::DesktopEvent).
//!
//! Linux has a StatusNotifierItem over D-Bus (`ksni`). macOS and Windows
//! have no tray yet (plan step 13b): the window always shows and closing it
//! quits.

use std::sync::Arc;

use super::Events;
use crate::ui::plugin::PluginMessage;

/// What choosing a tray menu item does.
#[derive(Debug, Clone)]
pub enum TrayCommand {
    /// Show the window.
    Open,
    /// Show the window on Settings.
    Settings,
    Quit,
    /// Show the window on this device's page.
    ShowDevice(String),
    /// A plugin's device action, run without showing the window.
    Action(PluginMessage),
}

/// One entry of the tray menu.
#[derive(Debug, Clone)]
pub enum TrayItem {
    /// A labelled item, shown disabled without a command.
    Item {
        label: String,
        command: Option<TrayCommand>,
    },
    Submenu {
        label: String,
        items: Vec<TrayItem>,
    },
    Separator,
}

impl TrayItem {
    pub fn item(label: impl Into<String>, command: Option<TrayCommand>) -> Self {
        Self::Item {
            label: label.into(),
            command,
        }
    }
}

/// What a menu shows, depth first: each entry's depth, label, and whether
/// it is enabled. Two menus with the same layout look the same, so the tray
/// isn't rebuilt for every transfer's progress.
pub fn layout(items: &[TrayItem]) -> Vec<(usize, Option<&str>, bool)> {
    fn walk<'a>(
        items: &'a [TrayItem],
        depth: usize,
        out: &mut Vec<(usize, Option<&'a str>, bool)>,
    ) {
        for item in items {
            match item {
                TrayItem::Item { label, command } => {
                    out.push((depth, Some(label), command.is_some()));
                }
                TrayItem::Submenu { label, items } => {
                    out.push((depth, Some(label), true));
                    walk(items, depth + 1, out);
                }
                TrayItem::Separator => out.push((depth, None, false)),
            }
        }
    }
    let mut out = Vec::new();
    walk(items, 0, &mut out);
    out
}

/// A tray icon whose menu the shell sets.
pub trait Tray: Send + Sync + 'static {
    /// Replace the menu.
    fn set_menu(&self, menu: Vec<TrayItem>);
}

/// No tray: nothing to show a menu in.
pub struct NoTray;

impl Tray for NoTray {
    fn set_menu(&self, _menu: Vec<TrayItem>) {}
}

/// The platform's tray, reporting to `events`, and whether a tray host
/// shows it now. Later changes arrive as
/// [`DesktopEvent::TrayAvailable`](super::DesktopEvent::TrayAvailable).
pub fn spawn(runtime: &tokio::runtime::Handle, events: Events) -> (Arc<dyn Tray>, bool) {
    #[cfg(target_os = "linux")]
    {
        match sni::spawn(runtime, events) {
            Ok((tray, available)) => (Arc::new(tray), available),
            Err(error) => {
                tracing::warn!(%error, "no tray icon");
                (Arc::new(NoTray), false)
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (runtime, events);
        (Arc::new(NoTray), false)
    }
}

/// A StatusNotifierItem, through `ksni`. Spawned assuming a tray host, so
/// one that starts, stops or restarts later is followed
/// (`watcher_online`/`watcher_offline`).
#[cfg(target_os = "linux")]
mod sni {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use ksni::{
        MenuItem, TrayMethods,
        menu::{StandardItem, SubMenu},
    };
    use tokio::sync::watch;

    use super::{super::DesktopEvent, Events, Tray, TrayItem};

    const ICON: &[u8] = include_bytes!("../../../assets/tray_icon.png");

    struct Sni {
        menu: Vec<TrayItem>,
        icon: Vec<ksni::Icon>,
        events: Events,
        online: Arc<AtomicBool>,
    }

    impl ksni::Tray for Sni {
        fn id(&self) -> String {
            "myconnect".into()
        }

        fn title(&self) -> String {
            "MyConnect".into()
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icon.clone()
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: "MyConnect".into(),
                ..ksni::ToolTip::default()
            }
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            let _ = self.events.send(DesktopEvent::TrayClicked);
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            self.menu.iter().map(menu_item).collect()
        }

        fn watcher_online(&self) {
            self.online.store(true, Ordering::SeqCst);
            let _ = self.events.send(DesktopEvent::TrayAvailable(true));
        }

        fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
            tracing::info!(?reason, "no tray host");
            self.online.store(false, Ordering::SeqCst);
            let _ = self.events.send(DesktopEvent::TrayAvailable(false));
            // Keep the item, to show again when a host starts.
            true
        }
    }

    fn menu_item(item: &TrayItem) -> MenuItem<Sni> {
        match item {
            TrayItem::Item { label, command } => {
                let command = command.clone();
                StandardItem {
                    label: escape(label),
                    enabled: command.is_some(),
                    activate: Box::new(move |tray: &mut Sni| {
                        if let Some(command) = &command {
                            let _ = tray.events.send(DesktopEvent::TrayChose(command.clone()));
                        }
                    }),
                    ..StandardItem::default()
                }
                .into()
            }
            TrayItem::Submenu { label, items } => SubMenu {
                label: escape(label),
                submenu: items.iter().map(menu_item).collect(),
                ..SubMenu::default()
            }
            .into(),
            TrayItem::Separator => MenuItem::Separator,
        }
    }

    /// A single underscore marks an access key in a D-Bus menu label, so a
    /// device named "my_phone" would lose it.
    fn escape(label: &str) -> String {
        label.replace('_', "__")
    }

    /// The icon as ARGB32 in network byte order, which the item wants.
    fn icon() -> Vec<ksni::Icon> {
        let image = match image::load_from_memory(ICON) {
            Ok(image) => image.into_rgba8(),
            Err(error) => {
                tracing::warn!(%error, "the tray icon doesn't decode");
                return Vec::new();
            }
        };
        let (width, height) = image.dimensions();
        let data = image
            .pixels()
            .flat_map(|pixel| {
                let [r, g, b, a] = pixel.0;
                [a, r, g, b]
            })
            .collect();
        vec![ksni::Icon {
            width: width as i32,
            height: height as i32,
            data,
        }]
    }

    pub struct SniTray {
        menu: watch::Sender<Vec<TrayItem>>,
    }

    impl Tray for SniTray {
        fn set_menu(&self, menu: Vec<TrayItem>) {
            self.menu.send_replace(menu);
        }
    }

    pub fn spawn(
        runtime: &tokio::runtime::Handle,
        events: Events,
    ) -> Result<(SniTray, bool), ksni::Error> {
        let online = Arc::new(AtomicBool::new(true));
        let tray = Sni {
            menu: Vec::new(),
            icon: icon(),
            events,
            online: online.clone(),
        };
        // A missing tray host is reported to `watcher_offline` before this
        // returns.
        let handle = runtime.block_on(tray.assume_sni_available(true).spawn())?;
        let (menu, mut changed) = watch::channel(Vec::new());
        // Menus are applied one at a time, in order, the latest winning.
        runtime.spawn(async move {
            while changed.changed().await.is_ok() {
                let menu = changed.borrow_and_update().clone();
                if handle.update(|tray| tray.menu = menu).await.is_none() {
                    return;
                }
            }
        });
        Ok((SniTray { menu }, online.load(Ordering::SeqCst)))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn labels_keep_their_underscores() {
            assert_eq!(escape("my_phone"), "my__phone");
        }

        #[test]
        fn the_icon_decodes() {
            let [icon] = &icon()[..] else {
                panic!("one icon");
            };
            assert_eq!((icon.width, icon.height), (64, 64));
            assert_eq!(icon.data.len(), 64 * 64 * 4);
        }
    }
}
