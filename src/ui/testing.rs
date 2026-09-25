//! Helpers for the UI's tests.

use std::{fs, path::Path};

use iced::{Element, Settings, Size, Theme};
use iced_test::simulator::Simulator;

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
