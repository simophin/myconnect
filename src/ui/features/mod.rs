//! Each feature's UI: its actions, status, pages and settings.

pub mod battery;
pub mod browse;
pub mod clipboard;
pub mod findmyphone;
pub mod ping;
pub mod share;

use std::sync::Arc;

use crate::{
    plugins::{browse::BrowsePlugin, clipboard::ClipboardPlugin},
    ui::plugin::ErasedUiPlugin,
};

/// Every feature's UI, in `plugins::builtin` order, over the plugin
/// instances the core runs.
pub(crate) fn all(
    clipboard: Arc<ClipboardPlugin>,
    browse: Arc<BrowsePlugin>,
) -> Vec<Box<dyn ErasedUiPlugin>> {
    vec![
        Box::new(ping::PingUi),
        Box::new(findmyphone::FindMyPhoneUi),
        Box::new(battery::BatteryUi),
        Box::new(clipboard::ClipboardUi::new(clipboard)),
        Box::new(share::ShareUi),
        Box::new(browse::BrowseUi::new(browse)),
    ]
}
