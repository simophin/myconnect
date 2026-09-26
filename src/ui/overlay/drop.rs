//! Files dropped on the window: the drag in progress, the hint drawn while
//! it hovers, and the chooser that asks which device a drop away from one
//! is for.
//!
//! No platform says where a drag is while it hovers (ADR 0001, "Desktop
//! integration"), so a drop isn't hit-tested: it goes to the page's device
//! if a feature takes it there, and otherwise to the chooser.

use std::{mem, path::PathBuf, time::Duration};

use iced::{
    Alignment, Background, Border, Element, Length, Theme,
    widget::{button, column, container, row, scrollable, space, stack, text},
};
use iced_fonts::lucide;

use super::dialog::surface;
use crate::{
    core::DeviceSnapshot,
    ui::{error::file_name, pages::devices::device_icon, widgets},
};

/// How long to wait for the rest of a drop's files once the first has
/// arrived, if the drag didn't say how many there are.
pub const SETTLE: Duration = Duration::from_millis(200);

/// Files dragged over the window, and those dropped so far.
///
/// winit reports each file on its own: a `FileHovered` per file as the drag
/// enters, then a `FileDropped` per file. A drop is complete once as many
/// have dropped as hovered; a drop with no count (or a short one) is
/// complete once [`SETTLE`] has passed.
#[derive(Debug, Default)]
pub struct Drag {
    active: bool,
    hovered: usize,
    dropped: Vec<PathBuf>,
    /// Which drag this is, so an older drag's timer doesn't end a newer one.
    gesture: u64,
}

/// What the shell does after a file drops.
#[derive(Debug, PartialEq, Eq)]
pub enum Dropped {
    /// Wait for more files.
    Wait,
    /// Wait [`SETTLE`], then call [`Drag::settled`] with this gesture.
    Settle(u64),
    /// Every file is in.
    Done(Vec<PathBuf>),
}

impl Drag {
    /// Files are over the window.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// One more file is over the window.
    pub fn hover(&mut self) {
        self.start();
        self.hovered += 1;
    }

    /// The files left the window without dropping.
    pub fn leave(&mut self) {
        self.finish();
    }

    /// A file dropped.
    pub fn drop_file(&mut self, path: PathBuf) -> Dropped {
        self.start();
        self.dropped.push(path);
        if self.hovered > 0 && self.dropped.len() >= self.hovered {
            Dropped::Done(self.finish())
        } else if self.dropped.len() == 1 {
            Dropped::Settle(self.gesture)
        } else {
            Dropped::Wait
        }
    }

    /// The wait for `gesture`'s files is over: what dropped, unless the
    /// drop already completed.
    pub fn settled(&mut self, gesture: u64) -> Option<Vec<PathBuf>> {
        (self.active && gesture == self.gesture && !self.dropped.is_empty()).then(|| self.finish())
    }

    fn start(&mut self) {
        if !self.active {
            self.active = true;
            self.gesture += 1;
            self.hovered = 0;
            self.dropped.clear();
        }
    }

    fn finish(&mut self) -> Vec<PathBuf> {
        self.active = false;
        self.hovered = 0;
        mem::take(&mut self.dropped)
    }
}

/// `base` with the window outlined and a pill saying what a drop does.
pub fn hint<'a, M: 'a>(base: Element<'a, M>, label: String) -> Element<'a, M> {
    let pill = container(
        row![lucide::file_up().size(18), text(label).size(14)]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .padding([10, 18])
    .style(|theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(Background::Color(palette.primary.base.color)),
            text_color: Some(palette.primary.base.text),
            border: Border::default().rounded(24),
            ..container::Style::default()
        }
    });
    let outline = container(pill)
        .padding(24)
        .center_x(Length::Fill)
        .align_bottom(Length::Fill)
        .style(|theme: &Theme| container::Style {
            border: Border::default()
                .width(3)
                .color(theme.extended_palette().primary.base.color),
            ..container::Style::default()
        });
    stack![base, outline].into()
}

/// What the hint says when the page has no device to drop on.
pub const CHOOSE_LABEL: &str = "Drop anywhere to choose a device";

/// Asks which device to send `paths` to, from `devices`, the paired devices
/// that would take them now. `choose` makes the message for a device's id.
pub fn chooser<'a, M: Clone + 'a>(
    paths: &[PathBuf],
    devices: Vec<&'a DeviceSnapshot>,
    choose: fn(String) -> M,
    cancel: M,
) -> Element<'a, M> {
    // The title stays short; a file's name, which can be any length, goes
    // under it and wraps.
    let (title, name) = match paths {
        [path] => ("Send file".to_owned(), Some(file_name(path))),
        _ => (format!("Send {} files", paths.len()), None),
    };
    let body: Element<'a, M> = if devices.is_empty() {
        text("No paired device is connected and able to receive files.")
            .size(14)
            .into()
    } else {
        let rows = devices.into_iter().map(|device| {
            button(
                row![
                    device_icon(device.device_type).size(20),
                    text(&device.device_name)
                        .size(15)
                        .wrapping(text::Wrapping::WordOrGlyph),
                ]
                .spacing(14)
                .align_y(Alignment::Center),
            )
            .padding([10, 12])
            .width(Length::Fill)
            .style(widgets::card_button)
            .on_press(choose(device.device_id.clone()))
            .into()
        });
        column![
            text("Choose the device to send to:").size(14),
            scrollable(column(rows).spacing(6)).height(Length::Shrink),
        ]
        .spacing(12)
        .into()
    };
    surface(
        column![
            column![text(title).size(20).font(widgets::bold())]
                .push(name.map(|name| {
                    text(name)
                        .size(14)
                        .style(text::secondary)
                        .wrapping(text::Wrapping::WordOrGlyph)
                }))
                .spacing(4),
            body,
            row![
                space::horizontal(),
                button(text("Cancel"))
                    .padding([8, 14])
                    .style(button::text)
                    .on_press(cancel),
            ],
        ]
        .spacing(16),
    )
}

#[cfg(test)]
mod tests {
    use iced_test::simulator::Simulator;

    use super::*;
    use crate::ui::testing;

    #[test]
    fn a_drop_is_complete_once_every_hovered_file_has_dropped() {
        let mut drag = Drag::default();
        drag.hover();
        drag.hover();
        assert!(drag.is_active());
        let gesture = match drag.drop_file("/a".into()) {
            Dropped::Settle(gesture) => gesture,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            drag.drop_file("/b".into()),
            Dropped::Done(vec!["/a".into(), "/b".into()])
        );
        assert!(!drag.is_active());
        // Its timer finds nothing left.
        assert_eq!(drag.settled(gesture), None);
    }

    #[test]
    fn a_drop_without_a_count_is_complete_once_it_settles() {
        let mut drag = Drag::default();
        let Dropped::Settle(gesture) = drag.drop_file("/a".into()) else {
            panic!("waits for more");
        };
        assert_eq!(drag.drop_file("/b".into()), Dropped::Wait);
        assert_eq!(drag.settled(gesture), Some(vec!["/a".into(), "/b".into()]));
    }

    #[test]
    fn leaving_forgets_the_drag_and_an_old_timer_leaves_a_new_one_alone() {
        let mut drag = Drag::default();
        drag.hover();
        drag.leave();
        assert!(!drag.is_active());

        drag.hover();
        drag.hover();
        let Dropped::Settle(first) = drag.drop_file("/a".into()) else {
            panic!("waits for more");
        };
        drag.leave();
        drag.hover();
        drag.hover();
        let Dropped::Settle(second) = drag.drop_file("/b".into()) else {
            panic!("a new drag");
        };
        assert_eq!(drag.settled(first), None);
        assert_eq!(drag.settled(second), Some(vec!["/b".into()]));
    }

    fn capable(name: &str) -> DeviceSnapshot {
        testing::device(name)
    }

    #[test]
    fn the_chooser_names_the_files_and_lists_the_devices() {
        let pixel = capable("Pixel");
        let laptop = capable("Laptop");
        let photo = [PathBuf::from("/tmp/photo.jpg")];
        let mut ui = Simulator::new(chooser(&photo, vec![&pixel, &laptop], Some, None));
        assert!(ui.find("Send file").is_ok());
        assert!(ui.find("photo.jpg").is_ok());
        ui.click("Laptop").unwrap();
        ui.click("Cancel").unwrap();
        let asked: Vec<_> = ui.into_messages().collect();
        assert_eq!(asked, [Some(laptop.device_id.clone()), None]);

        let two = [PathBuf::from("/a"), PathBuf::from("/b")];
        let mut ui = Simulator::new(chooser(&two, Vec::new(), Some, None::<String>));
        assert!(ui.find("Send 2 files").is_ok());
        assert!(
            ui.find("No paired device is connected and able to receive files.")
                .is_ok()
        );
    }

    #[test]
    fn snapshot_drop() {
        let pixel = capable("Pixel 8a");
        let mut laptop = capable("Work laptop");
        laptop.device_type = crate::protocol::DeviceType::Laptop;
        let paths = [PathBuf::from("/a"), PathBuf::from("/b")];
        testing::snapshot("drop-chooser", (440.0, 620.0), || {
            super::super::dialog::modal(
                container(text("page")).center(Length::Fill).into(),
                chooser(&paths, vec![&pixel, &laptop], |_| (), ()),
                None,
            )
        });
        // A long name wraps under the title, even without spaces.
        let long = [PathBuf::from(
            "/Holiday_photos_from_the_trip_to_the_mountains_in_the_summer_of_2025_final_edit.jpg",
        )];
        testing::snapshot("drop-chooser-long-name", (440.0, 620.0), || {
            super::super::dialog::modal(
                container(text("page")).center(Length::Fill).into(),
                chooser(&long, Vec::new(), |_| (), ()),
                None,
            )
        });
        testing::snapshot("drop-hint", (440.0, 620.0), || {
            hint::<()>(
                container(text("page")).center(Length::Fill).into(),
                CHOOSE_LABEL.into(),
            )
        });
    }
}
