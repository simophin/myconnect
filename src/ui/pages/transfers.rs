//! File transfers: the Transfers page, and one row per transfer, which the
//! device page uses for its recent ones.

use std::path::PathBuf;

use iced::{
    Alignment, Element, Length,
    widget::{column, container, progress_bar, row, scrollable, text},
};
use iced_fonts::lucide;
use uuid::Uuid;

use crate::{
    core::{OperationErrorCode, TransferDirection, TransferSnapshot, TransferStatus},
    ui::{
        activity::activity_bar,
        store::{Load, Store},
        widgets::{self, format_bytes},
    },
};

/// What a transfer row's buttons ask for.
pub struct Actions<M> {
    /// Stop the transfer of this id.
    pub cancel: fn(Uuid) -> M,
    /// Open a received file with its default app.
    pub open: fn(PathBuf) -> M,
    /// Show a received file in the file manager.
    pub reveal: fn(PathBuf) -> M,
}

/// Every transfer since the daemon started, newest first, from what `store`
/// holds. `retry` reads the core again after a failed snapshot.
pub fn view<'a, M: Clone + 'a>(
    store: &'a Store,
    actions: &Actions<M>,
    back: M,
    retry: M,
) -> Element<'a, M> {
    let body: Element<'a, M> = match store.transfers(None) {
        Load::Loading => widgets::loading("Loading transfers…"),
        Load::Failed(error) => widgets::error_view(error, Some(retry)),
        Load::Loaded(transfers) if transfers.is_empty() => {
            widgets::empty_state(lucide::arrow_up_down, "No transfers yet", None, None)
        }
        Load::Loaded(transfers) => {
            let rows = transfers
                .into_iter()
                .map(|transfer| widgets::card(transfer_row(transfer, true, actions)).into());
            scrollable(column(rows).spacing(8))
                .spacing(6)
                .height(Length::Fill)
                .into()
        }
    };
    widgets::page(widgets::page_header("Transfers", Some(back), vec![]), body)
}

/// One transfer: its direction, file, and progress or outcome, with Cancel
/// while it runs and Open file and Open folder once a received file is
/// saved. With `show_device`, it names the other device, for lists that mix
/// devices.
pub fn transfer_row<'a, M: Clone + 'a>(
    transfer: &'a TransferSnapshot,
    show_device: bool,
    actions: &Actions<M>,
) -> Element<'a, M> {
    let incoming = transfer.direction == TransferDirection::Incoming;
    let peer = if show_device {
        let preposition = if incoming { "From" } else { "To" };
        format!("{preposition} {} · ", transfer.device_name)
    } else {
        String::new()
    };
    let icon = if incoming {
        lucide::download()
    } else {
        lucide::upload()
    };
    let mut details = column![
        text(&transfer.file_name).wrapping(text::Wrapping::None),
        text(format!("{peer}{}", status_label(transfer)))
            .size(13)
            .style(text::secondary),
    ]
    .spacing(2);
    if !transfer.status.is_terminal() {
        details = details.push(container(progress_view(transfer)).padding([4, 0]));
    }
    let mut line = row![
        icon.size(18).style(text::secondary),
        container(details).width(Length::Fill).clip(true),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    if !transfer.status.is_terminal() {
        line = line.push(widgets::icon_button(
            lucide::x,
            "Cancel",
            Some((actions.cancel)(transfer.id)),
        ));
    } else if transfer.status == TransferStatus::Completed
        && let Some(path) = &transfer.saved_path
    {
        line = line
            .push(widgets::icon_button(
                lucide::external_link,
                "Open file",
                Some((actions.open)(path.clone())),
            ))
            .push(widgets::icon_button(
                lucide::folder_open,
                "Open folder",
                Some((actions.reveal)(path.clone())),
            ));
    }
    line.into()
}

/// The bar under a running transfer: how far it is while bytes move, and
/// a sweep before that, while it waits or connects.
fn progress_view<'a, M: 'a>(transfer: &TransferSnapshot) -> Element<'a, M> {
    if transfer.status == TransferStatus::Transferring {
        progress_bar(0.0..=1.0, progress(transfer)).girth(4).into()
    } else {
        activity_bar(Length::Fill, 4.0).into()
    }
}

/// How far along the bar is: the share sent while transferring, and empty
/// before that (the bar sweeps then instead).
fn progress(transfer: &TransferSnapshot) -> f32 {
    if transfer.status != TransferStatus::Transferring || transfer.total_bytes == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let share = transfer.transferred_bytes as f64 / transfer.total_bytes as f64;
    #[allow(clippy::cast_possible_truncation)]
    let share = share.clamp(0.0, 1.0) as f32;
    share
}

/// A transfer's state in words, as the Flutter app put it
/// (`transfer_tile.dart`).
pub fn status_label(transfer: &TransferSnapshot) -> String {
    match transfer.status {
        TransferStatus::Queued => "Waiting".into(),
        TransferStatus::Connecting => "Connecting".into(),
        TransferStatus::Transferring => format!(
            "{} of {}",
            format_bytes(transfer.transferred_bytes),
            format_bytes(transfer.total_bytes)
        ),
        TransferStatus::Completed => format_bytes(transfer.total_bytes),
        TransferStatus::Cancelled => "Cancelled".into(),
        TransferStatus::Failed => match transfer.error_code {
            Some(OperationErrorCode::ConnectionFailed) => "Failed: connection lost",
            Some(OperationErrorCode::TimedOut) => "Failed: timed out",
            Some(OperationErrorCode::Unavailable) => "Failed: refused by the receiver",
            Some(OperationErrorCode::ProtocolError) => {
                "Failed: the device sent something unexpected"
            }
            Some(OperationErrorCode::Internal) | None => "Failed",
        }
        .into(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::Path;

    use iced::widget;
    use iced_test::simulator::Simulator;

    use super::*;
    use crate::ui::{store::Snapshot, testing};

    /// A transfer of `file_name` with a phone named "Pixel".
    pub(crate) fn transfer(
        file_name: &str,
        direction: TransferDirection,
        status: TransferStatus,
        created_at: u64,
    ) -> TransferSnapshot {
        TransferSnapshot {
            id: Uuid::new_v4(),
            device_id: format!("{:0<32}", "Pixel"),
            device_name: "Pixel".into(),
            direction,
            status,
            file_name: file_name.into(),
            total_bytes: 3 * 1024 * 1024,
            transferred_bytes: if status == TransferStatus::Transferring {
                1024 * 1024
            } else {
                0
            },
            created_at,
            updated_at: created_at,
            error_code: None,
            saved_path: None,
        }
    }

    #[test]
    fn statuses_read_like_the_flutter_app() {
        let mut row = transfer(
            "a.jpg",
            TransferDirection::Incoming,
            TransferStatus::Queued,
            1,
        );
        let mut says = |status, error_code| {
            row.status = status;
            row.error_code = error_code;
            row.transferred_bytes = 1024 * 1024;
            status_label(&row)
        };
        assert_eq!(says(TransferStatus::Queued, None), "Waiting");
        assert_eq!(says(TransferStatus::Connecting, None), "Connecting");
        assert_eq!(says(TransferStatus::Transferring, None), "1.0 MB of 3.0 MB");
        assert_eq!(says(TransferStatus::Completed, None), "3.0 MB");
        assert_eq!(says(TransferStatus::Cancelled, None), "Cancelled");
        for (code, words) in [
            (None, "Failed"),
            (Some(OperationErrorCode::Internal), "Failed"),
            (
                Some(OperationErrorCode::ConnectionFailed),
                "Failed: connection lost",
            ),
            (Some(OperationErrorCode::TimedOut), "Failed: timed out"),
            (
                Some(OperationErrorCode::Unavailable),
                "Failed: refused by the receiver",
            ),
            (
                Some(OperationErrorCode::ProtocolError),
                "Failed: the device sent something unexpected",
            ),
        ] {
            assert_eq!(says(TransferStatus::Failed, code), words);
        }
    }

    #[test]
    fn progress_fills_only_while_transferring() {
        let mut row = transfer(
            "a.jpg",
            TransferDirection::Outgoing,
            TransferStatus::Transferring,
            1,
        );
        assert!((progress(&row) - 1.0 / 3.0).abs() < 1e-6);
        row.status = TransferStatus::Connecting;
        assert!(progress(&row).abs() < f32::EPSILON);
    }

    /// What the page asks for, in tests.
    #[derive(Debug, Clone, PartialEq)]
    enum Asked {
        Back,
        Retry,
        Cancel(Uuid),
        Open(PathBuf),
        Reveal(PathBuf),
    }

    const ACTIONS: Actions<Asked> = Actions {
        cancel: Asked::Cancel,
        open: Asked::Open,
        reveal: Asked::Reveal,
    };

    fn page(store: &Store) -> Element<'_, Asked> {
        view(store, &ACTIONS, Asked::Back, Asked::Retry)
    }

    fn with(transfers: Vec<TransferSnapshot>) -> Store {
        let mut store = testing::store("Desk", Vec::new());
        let settings = store.settings().loaded().cloned();
        store.apply_snapshot(Snapshot {
            devices: Ok(Vec::new()),
            pairings: Ok(Vec::new()),
            transfers,
            settings: settings.ok_or_else(String::new),
        });
        store
    }

    /// A running download of a movie, a received note saved to Downloads,
    /// and a photo sent, newest first.
    fn mixed() -> (Store, TransferSnapshot, PathBuf) {
        let running = transfer(
            "movie.mkv",
            TransferDirection::Incoming,
            TransferStatus::Transferring,
            3,
        );
        let saved = PathBuf::from("/home/me/Downloads/notes.txt");
        let mut received = transfer(
            "notes.txt",
            TransferDirection::Incoming,
            TransferStatus::Completed,
            2,
        );
        received.saved_path = Some(saved.clone());
        let sent = transfer(
            "photo.jpg",
            TransferDirection::Outgoing,
            TransferStatus::Completed,
            1,
        );
        (with(vec![sent, received, running.clone()]), running, saved)
    }

    #[test]
    fn lists_every_transfer_with_its_device_and_state() {
        let (store, _, _) = mixed();
        let mut ui = Simulator::new(page(&store));
        for shown in [
            "Transfers",
            "movie.mkv",
            "From Pixel · 1.0 MB of 3.0 MB",
            "notes.txt",
            "From Pixel · 3.0 MB",
            "photo.jpg",
            "To Pixel · 3.0 MB",
        ] {
            assert!(ui.find(shown).is_ok(), "{shown} is shown");
        }
    }

    #[test]
    fn newest_comes_first() {
        let (store, _, _) = mixed();
        let Load::Loaded(transfers) = store.transfers(None) else {
            panic!("loaded");
        };
        let names: Vec<_> = transfers.iter().map(|t| t.file_name.as_str()).collect();
        assert_eq!(names, ["movie.mkv", "notes.txt", "photo.jpg"]);
    }

    #[test]
    fn a_running_transfer_can_be_cancelled() {
        let (store, running, _) = mixed();
        let mut ui = Simulator::new(page(&store));
        ui.click(widget::Id::from("Cancel")).unwrap();
        assert_eq!(
            ui.into_messages().collect::<Vec<_>>(),
            [Asked::Cancel(running.id)]
        );
    }

    #[test]
    fn a_saved_file_opens_and_shows_in_its_folder() {
        let (store, _, saved) = mixed();
        let mut ui = Simulator::new(page(&store));
        ui.click(widget::Id::from("Open file")).unwrap();
        ui.click(widget::Id::from("Open folder")).unwrap();
        assert_eq!(
            ui.into_messages().collect::<Vec<_>>(),
            [Asked::Open(saved.clone()), Asked::Reveal(saved)]
        );
    }

    #[test]
    fn only_running_transfers_cancel_and_only_saved_files_open() {
        let mut received = transfer(
            "notes.txt",
            TransferDirection::Incoming,
            TransferStatus::Completed,
            1,
        );
        let unsaved = with(vec![received.clone()]);
        let mut ui = Simulator::new(page(&unsaved));
        for absent in ["Cancel", "Open file", "Open folder"] {
            assert!(ui.find(widget::Id::from(absent)).is_err(), "{absent}");
        }

        received.status = TransferStatus::Failed;
        received.saved_path = Some(Path::new("/home/me/Downloads/notes.txt").into());
        let failed = with(vec![received]);
        let mut ui = Simulator::new(page(&failed));
        for absent in ["Cancel", "Open file", "Open folder"] {
            assert!(ui.find(widget::Id::from(absent)).is_err(), "{absent}");
        }
    }

    #[test]
    fn nothing_yet_says_so_and_back_goes_back() {
        let store = with(Vec::new());
        let mut ui = Simulator::new(page(&store));
        assert!(ui.find("No transfers yet").is_ok());
        ui.click(widget::Id::from("Back")).unwrap();
        assert_eq!(ui.into_messages().collect::<Vec<_>>(), [Asked::Back]);

        let loading = Store::default();
        let mut ui = Simulator::new(page(&loading));
        assert!(ui.find("Loading transfers…").is_ok());
    }

    #[test]
    fn snapshot_transfers_page() {
        let (_, running, _) = mixed();
        let mut queued = transfer(
            "archive.zip",
            TransferDirection::Outgoing,
            TransferStatus::Connecting,
            4,
        );
        queued.device_name = "Galaxy Tab".into();
        let mut received = transfer(
            "notes.txt",
            TransferDirection::Incoming,
            TransferStatus::Completed,
            2,
        );
        received.saved_path = Some("/home/me/Downloads/notes.txt".into());
        let mut failed = transfer(
            "song.mp3",
            TransferDirection::Outgoing,
            TransferStatus::Failed,
            1,
        );
        failed.error_code = Some(OperationErrorCode::ConnectionFailed);
        let cancelled = transfer(
            "a-very-long-file-name-from-the-camera-roll-2026-09-25.heic",
            TransferDirection::Incoming,
            TransferStatus::Cancelled,
            0,
        );
        let store = with(vec![queued, running, received, failed, cancelled]);
        testing::snapshot("transfers", (440.0, 620.0), || page(&store));
        let empty = with(Vec::new());
        testing::snapshot("transfers-empty", (440.0, 360.0), || page(&empty));
    }
}
