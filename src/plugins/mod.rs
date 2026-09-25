//! The daemon's features.
//!
//! Each feature implements [`crate::core::Plugin`] and is listed in
//! [`builtin`]: ping, find my phone, battery, clipboard, share and browse.
//! The set is fixed at compile time; nothing is loaded at runtime. A plugin
//! reaches the core through its [`crate::core::PluginContext`], never another
//! plugin. See `docs/ARCHITECTURE.md` §2.

pub mod battery;
pub mod browse;
pub mod clipboard;
pub mod findmyphone;
pub mod ping;
pub mod share;

use std::sync::Arc;

use crate::core::Plugin;

/// Every plugin in this build. `clipboard` is the clipboard that clipboard
/// sync reads and writes: the desktop's, or an in-memory one.
pub fn builtin(
    clipboard: Arc<dyn clipboard::ClipboardService + Send + Sync>,
) -> Vec<Arc<dyn Plugin>> {
    vec![
        Arc::new(ping::PingPlugin),
        Arc::new(findmyphone::FindMyPhonePlugin),
        Arc::new(battery::BatteryPlugin::default()),
        Arc::new(clipboard::ClipboardPlugin::new(clipboard)),
        Arc::new(share::SharePlugin),
        Arc::new(browse::BrowsePlugin::default()),
    ]
}

/// The plugins [`builtin_with_ui`] builds: the core's list, and the UI
/// halves of the same instances.
#[cfg(feature = "gui")]
pub struct Builtin {
    pub core: Vec<Arc<dyn Plugin>>,
    pub ui: Vec<Box<dyn crate::ui::plugin::ErasedUiPlugin>>,
}

/// Every plugin in this build, as [`builtin`] lists them, plus the UI half
/// of each feature that has one, built from the same instance the core
/// runs. Keep both lists in [`builtin`]'s order: the UI shows each plugin's
/// actions and sections in this order.
#[cfg(feature = "gui")]
pub fn builtin_with_ui(clipboard: Arc<dyn clipboard::ClipboardService + Send + Sync>) -> Builtin {
    let clipboard = Arc::new(clipboard::ClipboardPlugin::new(clipboard));
    let browse = Arc::new(browse::BrowsePlugin::default());
    Builtin {
        core: vec![
            Arc::new(ping::PingPlugin),
            Arc::new(findmyphone::FindMyPhonePlugin),
            Arc::new(battery::BatteryPlugin::default()),
            clipboard.clone(),
            Arc::new(share::SharePlugin),
            browse.clone(),
        ],
        ui: vec![
            Box::new(ping::ui::PingUi),
            Box::new(findmyphone::ui::FindMyPhoneUi),
            Box::new(battery::ui::BatteryUi),
            Box::new(clipboard::ui::ClipboardUi::new(clipboard)),
            Box::new(share::ui::ShareUi),
            Box::new(browse::ui::BrowseUi::new(browse)),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::PluginRegistry;

    #[cfg(feature = "gui")]
    #[test]
    fn builtin_with_ui_runs_the_same_plugins_in_the_same_order() {
        fn ids(plugins: &[Arc<dyn Plugin>]) -> Vec<&'static str> {
            plugins.iter().map(|plugin| plugin.id()).collect()
        }
        let clipboard = clipboard::InMemoryClipboard::shared;
        let with_ui = builtin_with_ui(clipboard());
        let core = ids(&with_ui.core);
        assert_eq!(core, ids(&builtin(clipboard())));
        // Each UI half belongs to a core plugin, in the same order.
        let positions: Vec<_> = with_ui
            .ui
            .iter()
            .map(|plugin| {
                core.iter()
                    .position(|id| *id == plugin.id())
                    .expect("a core plugin")
            })
            .collect();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn advertises_ping_clipboard_and_share_both_directions_and_the_rest_one_way() {
        // Order doesn't matter to peers, so compare sorted lists.
        fn sorted(mut values: Vec<String>) -> Vec<String> {
            values.sort();
            values
        }
        fn strings(values: &[&str]) -> Vec<String> {
            sorted(values.iter().map(|value| value.to_string()).collect())
        }
        let capabilities =
            PluginRegistry::new(builtin(clipboard::InMemoryClipboard::shared())).capabilities();
        let bidirectional = [
            ping::PACKET_TYPE,
            clipboard::PACKET_TYPE,
            clipboard::CONNECT_PACKET_TYPE,
            share::PACKET_TYPE,
        ];
        assert_eq!(
            sorted(capabilities.incoming),
            strings(
                &[
                    &bidirectional[..],
                    &[browse::PACKET_TYPE, battery::PACKET_TYPE]
                ]
                .concat()
            )
        );
        assert_eq!(
            sorted(capabilities.outgoing),
            strings(
                &[
                    &bidirectional[..],
                    &[
                        browse::REQUEST_PACKET_TYPE,
                        findmyphone::REQUEST_PACKET_TYPE
                    ]
                ]
                .concat()
            )
        );
    }
}
