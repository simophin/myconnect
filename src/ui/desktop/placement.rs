//! Where the main window was, and whether it was showing, kept in
//! `window.json` (ADR 0009): the one thing the UI stores itself. It is
//! about this desktop's window rather than the device, and it is needed
//! before the daemon is up.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use iced::{Point, Rectangle, Size};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The main window as last seen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Showing, rather than closed to the tray. Minimized counts as
    /// showing.
    pub visible: bool,
    pub maximized: bool,
    /// Position and size when neither maximized nor minimized, or `None`
    /// before the window was first seen.
    pub bounds: Option<Rectangle>,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            visible: true,
            maximized: false,
            bounds: None,
        }
    }
}

/// The file's shape, the same as the Flutter app's.
#[derive(Serialize, Deserialize)]
struct Stored {
    visible: bool,
    #[serde(default)]
    maximized: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bounds: Option<Value>,
}

impl Placement {
    /// Read back from [`to_json`](Self::to_json)'s output, or `None` if
    /// `json` isn't that. Bounds that make no sense are dropped.
    fn from_json(json: &str) -> Option<Self> {
        let stored: Stored = serde_json::from_str(json).ok()?;
        let bounds = stored
            .bounds
            .and_then(|bounds| serde_json::from_value::<[f32; 4]>(bounds).ok())
            .filter(|[_, _, width, height]| *width > 0.0 && *height > 0.0)
            .map(|[x, y, width, height]| Rectangle {
                x,
                y,
                width,
                height,
            });
        Some(Self {
            visible: stored.visible,
            maximized: stored.maximized == Value::Bool(true),
            bounds,
        })
    }

    fn to_json(self) -> String {
        let stored = Stored {
            visible: self.visible,
            maximized: Value::Bool(self.maximized),
            bounds: self
                .bounds
                .map(|bounds| serde_json::json!([bounds.x, bounds.y, bounds.width, bounds.height])),
        };
        serde_json::to_string(&stored).expect("a placement serializes")
    }

    /// The placement after seeing the window: bounds are only taken from a
    /// normal window, since a maximized or minimized one isn't where it goes
    /// back to, and a position the platform can't tell (Wayland) keeps the
    /// old one.
    pub fn seen(self, seen: Seen, visible: bool) -> Self {
        let normal = !seen.maximized && !seen.minimized;
        let bounds = if normal {
            let position = seen
                .position
                .or(self.bounds.map(|bounds| bounds.position()))
                .unwrap_or(Point::ORIGIN);
            Some(Rectangle::new(position, seen.size))
        } else {
            self.bounds
        };
        Self {
            visible,
            maximized: seen.maximized,
            bounds,
        }
    }
}

/// What the platform tells about the open window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seen {
    /// `None` where the platform can't tell (Wayland).
    pub position: Option<Point>,
    pub size: Size,
    pub maximized: bool,
    pub minimized: bool,
}

/// Whether enough of a window at `bounds` would be on one of `screens` to
/// grab and move it, e.g. not on a monitor that has since been unplugged.
pub fn fits_on_screen(bounds: Rectangle, screens: &[Rectangle]) -> bool {
    screens.iter().any(|screen| {
        screen
            .intersection(&bounds)
            .is_some_and(|visible| visible.width >= 100.0 && visible.height >= 50.0)
    })
}

/// Keeps a [`Placement`] in a small JSON file.
#[derive(Debug, Clone)]
pub struct PlacementStore {
    file: PathBuf,
}

impl PlacementStore {
    pub fn new(file: PathBuf) -> Self {
        Self { file }
    }

    /// The file for this platform's state directory, or in
    /// `data_dir` when one is given, so a test or second instance doesn't
    /// share the owner's window.
    pub fn for_data_dir(data_dir: Option<&Path>) -> Self {
        let file = match data_dir {
            Some(data_dir) => data_dir.join("window.json"),
            None => default_dir().join("window.json"),
        };
        Self::new(file)
    }

    /// The saved placement, or `None` if there is none or it can't be read.
    pub fn load(&self) -> Option<Placement> {
        match fs::read_to_string(&self.file) {
            Ok(json) => {
                let placement = Placement::from_json(&json);
                if placement.is_none() {
                    tracing::warn!(file = %self.file.display(), "ignoring the saved window placement");
                }
                placement
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                tracing::warn!(%error, "ignoring the saved window placement");
                None
            }
        }
    }

    /// Replace the saved placement. Written to a temporary file first, so a
    /// crash mid-write leaves the old one.
    pub fn save(&self, placement: Placement) {
        if let Err(error) = self.write(placement) {
            tracing::warn!(%error, "could not save the window placement");
        }
    }

    fn write(&self, placement: Placement) -> io::Result<()> {
        if let Some(parent) = self.file.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut temporary = self.file.clone().into_os_string();
        temporary.push(".tmp");
        fs::write(&temporary, placement.to_json())?;
        fs::rename(&temporary, &self.file)
    }
}

/// Where `window.json` lives: XDG's state directory on Linux
/// (meant for things like window layout), Application Support on macOS,
/// and the local app data on Windows.
fn default_dir() -> PathBuf {
    let var = |name| env::var_os(name).filter(|value| !value.is_empty());
    let home = || PathBuf::from(var("HOME").unwrap_or_default());
    if cfg!(windows) {
        PathBuf::from(
            var("LOCALAPPDATA")
                .or_else(|| var("APPDATA"))
                .unwrap_or_else(|| ".".into()),
        )
        .join("Ferry")
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support/Ferry")
    } else {
        var("XDG_STATE_HOME")
            .map_or_else(|| home().join(".local/state"), PathBuf::from)
            .join("ferry")
    }
}

/// Every monitor's bounds, in logical pixels as iced places windows. Empty
/// if they can't be listed.
pub fn screens() -> Vec<Rectangle> {
    match display_info::DisplayInfo::all() {
        Ok(displays) => displays
            .into_iter()
            .map(|display| {
                let scale = if display.scale_factor > 0.0 {
                    display.scale_factor
                } else {
                    1.0
                };
                Rectangle {
                    x: display.x as f32 / scale,
                    y: display.y as f32 / scale,
                    width: display.width as f32 / scale,
                    height: display.height as f32 / scale,
                }
            })
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "could not list the monitors");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, PlacementStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = PlacementStore::new(dir.path().join("state/window.json"));
        (dir, store)
    }

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rectangle {
        Rectangle {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn nothing_is_saved_at_first() {
        let (_dir, store) = store();
        assert_eq!(store.load(), None);
    }

    #[test]
    fn a_saved_placement_reads_back_the_same() {
        let (_dir, store) = store();
        let placement = Placement {
            visible: false,
            maximized: true,
            bounds: Some(rect(120.0, 80.0, 900.0, 600.0)),
        };
        store.save(placement);
        assert_eq!(store.load(), Some(placement));

        let shown = Placement::default();
        store.save(shown);
        assert_eq!(store.load(), Some(shown));
    }

    #[test]
    fn an_unreadable_file_counts_as_nothing_saved() {
        let (_dir, store) = store();
        fs::create_dir_all(store.file.parent().unwrap()).unwrap();
        fs::write(&store.file, r#"{"visible": "yes""#).unwrap();
        assert_eq!(store.load(), None);
        fs::write(&store.file, r#"{"visible": true, "bounds": [1, 2, 0, 4]}"#).unwrap();
        assert_eq!(store.load(), Some(Placement::default()));
    }

    #[test]
    fn the_flutter_apps_file_reads() {
        let placement = Placement::from_json(
            r#"{"visible":false,"maximized":false,"bounds":[10.0,20.0,440.0,620.0]}"#,
        );
        assert_eq!(
            placement,
            Some(Placement {
                visible: false,
                maximized: false,
                bounds: Some(rect(10.0, 20.0, 440.0, 620.0)),
            })
        );
    }

    #[test]
    fn the_data_dir_holds_the_file_when_given() {
        let store = PlacementStore::for_data_dir(Some(Path::new("/tmp/run/data")));
        assert_eq!(store.file, Path::new("/tmp/run/data/window.json"));
    }

    #[test]
    fn bounds_come_only_from_a_normal_window() {
        let before = Placement {
            visible: true,
            maximized: false,
            bounds: Some(rect(10.0, 20.0, 400.0, 600.0)),
        };
        let normal = Seen {
            position: Some(Point::new(50.0, 60.0)),
            size: Size::new(500.0, 700.0),
            maximized: false,
            minimized: false,
        };
        assert_eq!(
            before.seen(normal, false),
            Placement {
                visible: false,
                maximized: false,
                bounds: Some(rect(50.0, 60.0, 500.0, 700.0)),
            }
        );
        let maximized = Seen {
            maximized: true,
            size: Size::new(1920.0, 1080.0),
            ..normal
        };
        assert_eq!(
            before.seen(maximized, true),
            Placement {
                maximized: true,
                ..before
            }
        );
        // Wayland: no position, so the old one stays.
        let unplaced = Seen {
            position: None,
            ..normal
        };
        assert_eq!(
            before.seen(unplaced, true).bounds,
            Some(rect(10.0, 20.0, 500.0, 700.0))
        );
    }

    #[test]
    fn fits_on_either_screen() {
        let left = rect(0.0, 0.0, 1920.0, 1080.0);
        let right = rect(1920.0, 0.0, 2560.0, 1440.0);
        assert!(fits_on_screen(rect(100.0, 100.0, 800.0, 600.0), &[left]));
        assert!(fits_on_screen(
            rect(2500.0, 300.0, 800.0, 600.0),
            &[left, right]
        ));
    }

    #[test]
    fn does_not_fit_on_a_screen_that_was_unplugged() {
        let left = rect(0.0, 0.0, 1920.0, 1080.0);
        assert!(!fits_on_screen(rect(2500.0, 300.0, 800.0, 600.0), &[left]));
    }

    #[test]
    fn does_not_fit_with_only_a_sliver_showing() {
        let left = rect(0.0, 0.0, 1920.0, 1080.0);
        assert!(!fits_on_screen(rect(1880.0, 100.0, 800.0, 600.0), &[left]));
    }
}
