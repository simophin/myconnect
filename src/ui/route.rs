//! Where the window is: the page it shows.

use uuid::Uuid;

/// A page of the window. Pages the UI core owns have their own variant; a
/// feature's pages are [`Route::Plugin`], drawn by that plugin's
/// `view_page`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// The home page: paired devices.
    Devices,
    /// One device, by id.
    Device(String),
    /// Scan for devices and pair with one.
    AddDevice,
    /// One pairing request, by id.
    Pairing(Uuid),
    /// Every file transfer.
    Transfers,
    Settings,
    /// A page a plugin owns, for one device.
    Plugin {
        /// The plugin's id.
        plugin: &'static str,
        device: String,
        /// Which of the plugin's pages; the plugin gives it meaning.
        page: String,
    },
}
