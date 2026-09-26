//! Where the window is: the page it shows.

use uuid::Uuid;

/// A page of the window.
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
    /// A device's files: a folder, or its storage (`None`).
    Browse {
        device: String,
        folder: Option<String>,
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
            Self::Browse { device, .. } => Some(Self::Device(device.clone())),
        }
    }
}

impl Route {
    /// The device this page is about, if any.
    pub fn device(&self) -> Option<&str> {
        match self {
            Self::Device(device) | Self::Browse { device, .. } => Some(device),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_walks_up_to_the_devices_page() {
        let files = Route::Browse {
            device: "phone".into(),
            folder: Some("/storage/emulated/0".into()),
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
