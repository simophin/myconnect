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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::PluginRegistry;

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
