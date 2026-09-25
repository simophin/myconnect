//! Helpers for the UI's tests.

use std::{fs, path::Path};

use iced::{Element, Settings, Size, Task, Theme, futures::StreamExt};
use iced_test::simulator::Simulator;

use crate::{
    core::{DeviceReachability, DeviceSnapshot, SettingsSnapshot},
    protocol::DeviceType,
    ui::store::{Snapshot, Store},
};

/// A paired, connected phone named `name`, with no capabilities.
pub fn device(name: &str) -> DeviceSnapshot {
    DeviceSnapshot {
        device_id: format!("{:0<32}", name.replace(' ', "")),
        device_name: name.into(),
        device_type: DeviceType::Phone,
        protocol_version: 8,
        incoming_capabilities: vec![],
        outgoing_capabilities: vec![],
        reachability: DeviceReachability::Connected,
        paired: true,
        pairing: false,
        last_seen_at: 0,
        plugins: Default::default(),
    }
}

/// A store holding `devices`, on a computer named `local_name`, with no
/// pairings or transfers.
pub fn store(local_name: &str, devices: Vec<DeviceSnapshot>) -> Store {
    let mut store = Store::default();
    store.apply_snapshot(Snapshot {
        devices: Ok(devices),
        pairings: Ok(Vec::new()),
        transfers: Vec::new(),
        settings: Ok(SettingsSnapshot {
            device_name: local_name.into(),
            download_dir: "/home/me/Downloads".into(),
            close_to_tray: true,
            plugins: Default::default(),
        }),
    });
    store
}

/// Run `task` to the end and return what it produced. Only for tasks that
/// just produce values: window and widget actions are dropped.
pub async fn outputs<T: 'static>(task: Task<T>) -> Vec<T> {
    let Some(stream) = iced_runtime::task::into_stream(task) else {
        return Vec::new();
    };
    stream
        .filter_map(async |action| match action {
            iced_runtime::Action::Output(output) => Some(output),
            _ => None,
        })
        .collect()
        .await
}

/// Render `view` headlessly, in light and dark, to
/// `$SNAPSHOT_DIR/<name>-<light|dark>-<backend>.png`, to look at the UI
/// without a display. Does nothing without the variable.
///
/// The snapshot font is not the app's, so judge layout, not typography.
pub fn snapshot<'a, Message>(
    name: &str,
    size: impl Into<Size> + Copy,
    view: impl Fn() -> Element<'a, Message>,
) {
    let Ok(directory) = std::env::var("SNAPSHOT_DIR") else {
        return;
    };
    let directory = Path::new(&directory);
    for (variant, theme) in [("light", Theme::Light), ("dark", Theme::Dark)] {
        let stem = format!("{name}-{variant}");
        remove_old_images(directory, &stem);
        let settings = Settings {
            fonts: vec![iced_fonts::LUCIDE_FONT_BYTES.into()],
            ..Settings::default()
        };
        let mut ui = Simulator::with_size(settings, size, view());
        let snapshot = ui.snapshot(&theme).expect("snapshot renders");
        assert!(
            snapshot
                .matches_image(directory.join(&stem))
                .expect("snapshot saves")
        );
    }
}

/// `matches_image` compares with an existing image instead of writing a new
/// one, and adds the renderer's name to the file name.
fn remove_old_images(directory: &Path, stem: &str) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let prefix = format!("{stem}-");
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".png") {
            let _ = fs::remove_file(entry.path());
        }
    }
}
