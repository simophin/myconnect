//! Opening, closing and reading the main window, behind a trait so tests
//! can see what the shell does with it: iced's window tasks only run in a
//! real event loop.

use iced::{Rectangle, Size, Task, window};

use super::placement::{self, Placement, Seen};

/// The main window's size when nothing is saved.
pub const DEFAULT_SIZE: Size = Size::new(440.0, 620.0);

pub trait Windows: Send + Sync + 'static {
    /// Open a window: its id now, and a task that ends once it is open.
    fn open(&self, settings: window::Settings) -> (window::Id, Task<()>);
    fn close(&self, id: window::Id) -> Task<()>;
    /// Bring the window to the front and focus it, unminimized.
    fn raise(&self, id: window::Id) -> Task<()>;
    /// What the platform tells about the window now.
    fn read(&self, id: window::Id) -> Task<Seen>;
    /// Every monitor's bounds.
    fn screens(&self) -> Vec<Rectangle>;
}

/// The real windows, through iced.
pub struct System;

impl Windows for System {
    fn open(&self, settings: window::Settings) -> (window::Id, Task<()>) {
        let (id, opened) = window::open(settings);
        (id, opened.discard())
    }

    fn close(&self, id: window::Id) -> Task<()> {
        window::close(id)
    }

    fn raise(&self, id: window::Id) -> Task<()> {
        window::minimize::<()>(id, false).chain(window::gain_focus(id))
    }

    fn read(&self, id: window::Id) -> Task<Seen> {
        window::position(id).then(move |position| {
            window::size(id).then(move |size| {
                window::is_maximized(id).then(move |maximized| {
                    window::is_minimized(id).map(move |minimized| Seen {
                        position,
                        size,
                        maximized,
                        minimized: minimized.unwrap_or(false),
                    })
                })
            })
        })
    }

    fn screens(&self) -> Vec<Rectangle> {
        placement::screens()
    }
}

/// The settings to open the main window with, put back where `placement`
/// says: at the same spot if it is still on one of `screens`, otherwise
/// centred at that size. Where the platform places windows itself
/// (Wayland), only the size and maximized state take.
pub fn settings(placement: &Placement, screens: &[Rectangle]) -> window::Settings {
    let (size, position) = match placement.bounds {
        Some(bounds) if placement::fits_on_screen(bounds, screens) => {
            (bounds.size(), window::Position::Specific(bounds.position()))
        }
        Some(bounds) => (bounds.size(), window::Position::Centered),
        None => (DEFAULT_SIZE, window::Position::default()),
    };
    window::Settings {
        size,
        position,
        maximized: placement.maximized,
        // The shell decides what closing means: to the tray, or quit.
        exit_on_close_request: false,
        ..window::Settings::default()
    }
}

#[cfg(test)]
mod tests {
    use iced::Point;

    use super::*;

    #[test]
    fn a_window_goes_back_where_it_was_or_is_centred() {
        let screen = Rectangle::new(Point::ORIGIN, Size::new(1920.0, 1080.0));
        let placement = Placement {
            visible: true,
            maximized: true,
            bounds: Some(Rectangle::new(
                Point::new(100.0, 50.0),
                Size::new(500.0, 700.0),
            )),
        };
        let restored = settings(&placement, &[screen]);
        assert_eq!(restored.size, Size::new(500.0, 700.0));
        assert!(matches!(
            restored.position,
            window::Position::Specific(Point { x: 100.0, y: 50.0 })
        ));
        assert!(restored.maximized);
        assert!(!restored.exit_on_close_request);

        let unplugged = settings(&placement, &[]);
        assert_eq!(unplugged.size, Size::new(500.0, 700.0));
        assert!(matches!(unplugged.position, window::Position::Centered));

        let fresh = settings(&Placement::default(), &[screen]);
        assert_eq!(fresh.size, DEFAULT_SIZE);
        assert!(!fresh.maximized);
    }
}
