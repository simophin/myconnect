//! The tray icon and its menu. The shell decides what the menu holds
//! ([`TrayItem`]s carrying [`TrayCommand`]s); the tray shows it and reports
//! what was chosen as a [`DesktopEvent`](super::DesktopEvent).
//!
//! Linux has a StatusNotifierItem over D-Bus (`ksni`); macOS and Windows
//! have `tray-icon` with `muda` menus, shown once the event loop runs
//! ([`Tray::start`]).

use std::sync::Arc;

use super::Events;
use crate::ui::features::Feature;

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
    /// A feature's device action, run without showing the window.
    Action(Feature),
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
    /// Show the icon. Called once, from `update` (on the main thread, with
    /// the event loop running), which is where macOS and Windows need their
    /// tray created.
    fn start(&self) {}

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
    #[cfg(any(target_os = "macos", windows))]
    {
        // Assumed to work; if it doesn't, `start` reports it.
        let _ = runtime;
        (Arc::new(native::NativeTray::new(events)), true)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = (runtime, events);
        (Arc::new(NoTray), false)
    }
}

/// A `tray-icon` status item with a `muda` menu. Neither is `Send` on macOS,
/// so they live in a thread local of the main thread, where `update` runs;
/// their events come from global handlers, forwarded to [`Events`].
#[cfg(any(target_os = "macos", windows))]
mod native {
    use std::{
        cell::RefCell,
        collections::HashMap,
        sync::{Arc, Mutex, PoisonError},
    };

    use tray_icon::{
        Icon, TrayIcon, TrayIconBuilder,
        menu::{IsMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu},
    };

    use super::{super::DesktopEvent, Events, Tray, TrayCommand, TrayItem};

    /// macOS draws a template image in the menu bar's colours.
    #[cfg(target_os = "macos")]
    const ICON: &[u8] = include_bytes!("../../../assets/tray_icon_template.png");
    #[cfg(windows)]
    const ICON: &[u8] = include_bytes!("../../../assets/tray_icon.png");

    thread_local! {
        static ICON_ITEM: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
    }

    type Commands = Arc<Mutex<HashMap<MenuId, TrayCommand>>>;

    pub struct NativeTray {
        events: Events,
        /// The latest menu, for `start` when it comes after `set_menu`.
        menu: Mutex<Vec<TrayItem>>,
        /// What each item of the menu shown does.
        commands: Commands,
    }

    impl NativeTray {
        pub fn new(events: Events) -> Self {
            Self {
                events,
                menu: Mutex::new(Vec::new()),
                commands: Commands::default(),
            }
        }

        /// A `muda` menu of `items`, recording what each item does.
        fn build(&self, items: &[TrayItem]) -> Menu {
            let mut commands = self.commands.lock().unwrap_or_else(PoisonError::into_inner);
            commands.clear();
            let menu = Menu::new();
            for item in items {
                append(&menu, item, &mut commands);
            }
            menu
        }
    }

    trait Append {
        fn append_item(&self, item: &dyn IsMenuItem);
    }

    impl Append for Menu {
        fn append_item(&self, item: &dyn IsMenuItem) {
            if let Err(error) = self.append(item) {
                tracing::warn!(%error, "a tray menu item wasn't added");
            }
        }
    }

    impl Append for Submenu {
        fn append_item(&self, item: &dyn IsMenuItem) {
            if let Err(error) = self.append(item) {
                tracing::warn!(%error, "a tray menu item wasn't added");
            }
        }
    }

    fn append(menu: &impl Append, item: &TrayItem, commands: &mut HashMap<MenuId, TrayCommand>) {
        match item {
            TrayItem::Item { label, command } => {
                let entry = MenuItem::new(escape(label), command.is_some(), None);
                if let Some(command) = command {
                    commands.insert(entry.id().clone(), command.clone());
                }
                menu.append_item(&entry);
            }
            TrayItem::Submenu { label, items } => {
                let submenu = Submenu::new(escape(label), true);
                for item in items {
                    append(&submenu, item, commands);
                }
                menu.append_item(&submenu);
            }
            TrayItem::Separator => menu.append_item(&PredefinedMenuItem::separator()),
        }
    }

    /// A single `&` marks a mnemonic in a `muda` label, so "Tom & Jerry"
    /// would lose it.
    pub(super) fn escape(label: &str) -> String {
        label.replace('&', "&&")
    }

    pub(super) fn icon() -> Option<Icon> {
        let image = match image::load_from_memory(ICON) {
            Ok(image) => image.into_rgba8(),
            Err(error) => {
                tracing::warn!(%error, "the tray icon doesn't decode");
                return None;
            }
        };
        let (width, height) = image.dimensions();
        Icon::from_rgba(image.into_raw(), width, height)
            .inspect_err(|error| tracing::warn!(%error, "the tray icon was refused"))
            .ok()
    }

    impl Tray for NativeTray {
        fn start(&self) {
            let commands = self.commands.clone();
            let events = self.events.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                let command = commands
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get(&event.id)
                    .cloned();
                if let Some(command) = command {
                    let _ = events.send(DesktopEvent::TrayChose(command));
                }
            }));
            // A left click opens the window and a right click the menu, as
            // on Linux. macOS opens the menu on either, as its own do.
            #[cfg(windows)]
            {
                use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
                let events = self.events.clone();
                TrayIconEvent::set_event_handler(Some(move |event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let _ = events.send(DesktopEvent::TrayClicked);
                    }
                }));
            }

            let menu = self
                .menu
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            let mut builder = TrayIconBuilder::new()
                .with_tooltip("MyConnect")
                .with_menu(Box::new(self.build(&menu)))
                .with_menu_on_left_click(cfg!(target_os = "macos"))
                .with_icon_as_template(cfg!(target_os = "macos"));
            if let Some(icon) = icon() {
                builder = builder.with_icon(icon);
            }
            match builder.build() {
                Ok(tray) => ICON_ITEM.with_borrow_mut(|item| *item = Some(tray)),
                Err(error) => {
                    tracing::warn!(%error, "no tray icon");
                    let _ = self.events.send(DesktopEvent::TrayAvailable(false));
                }
            }
        }

        fn set_menu(&self, menu: Vec<TrayItem>) {
            *self.menu.lock().unwrap_or_else(PoisonError::into_inner) = menu.clone();
            ICON_ITEM.with_borrow(|item| {
                if let Some(tray) = item {
                    tray.set_menu(Some(Box::new(self.build(&menu))));
                }
            });
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn labels_keep_their_ampersands() {
            assert_eq!(escape("Tom & Jerry"), "Tom && Jerry");
        }

        #[test]
        fn the_icon_decodes() {
            assert!(icon().is_some());
        }
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
