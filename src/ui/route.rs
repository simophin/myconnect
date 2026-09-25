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

impl Route {
    /// Where Back goes: the page this one was opened from, as the Flutter
    /// app's nested paths had it. The home page has none.
    pub fn parent(&self) -> Option<Route> {
        match self {
            Self::Devices => None,
            Self::Device(_) | Self::AddDevice | Self::Transfers | Self::Settings => {
                Some(Self::Devices)
            }
            Self::Pairing(_) => Some(Self::AddDevice),
            Self::Plugin { device, .. } => Some(Self::Device(device.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_walks_up_to_the_devices_page() {
        let files = Route::Plugin {
            plugin: "browse",
            device: "phone".into(),
            page: "files".into(),
        };
        let mut trail = vec![files.clone()];
        while let Some(parent) = trail.last().unwrap().parent() {
            trail.push(parent);
        }
        assert_eq!(
            trail,
            [files, Route::Device("phone".into()), Route::Devices]
        );
        assert_eq!(Route::Pairing(Uuid::nil()).parent(), Some(Route::AddDevice));
        assert_eq!(Route::Settings.parent(), Some(Route::Devices));
    }
}
