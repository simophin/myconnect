//! The daemon's features.
//!
//! Each feature implements [`crate::core::Plugin`] and is listed in
//! [`builtin`]: ping, find my phone, battery, clipboard, share and browse.
//! The set is fixed at compile time; nothing is loaded at runtime. See
//! `docs/research/feature-modules.md`.

pub mod battery;
pub mod browse;
pub mod clipboard;
pub mod findmyphone;
pub mod ping;
pub mod share;

use std::sync::Arc;

use crate::core::{Plugin, PluginRegistry};

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

/// Capability strings advertised by all packet handlers registered here.
/// These are copied verbatim into the `incomingCapabilities` and
/// `outgoingCapabilities` fields of the local identity packet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginCapabilities {
    pub incoming: Vec<String>,
    pub outgoing: Vec<String>,
}

/// The packet types this build can send and receive: those of the
/// [`builtin`] plugins.
pub fn capabilities() -> PluginCapabilities {
    let registry = PluginRegistry::new(builtin(clipboard::InMemoryClipboard::shared()));
    PluginCapabilities {
        incoming: registry.incoming().map(str::to_owned).collect(),
        outgoing: registry.outgoing().map(str::to_owned).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let capabilities = capabilities();
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
