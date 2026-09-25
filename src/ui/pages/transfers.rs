//! File transfers: one row per transfer, as the device page lists its
//! recent ones. The Transfers page itself is still to come.

use iced::{
    Alignment, Element, Length,
    widget::{column, container, progress_bar, row, text},
};
use iced_fonts::lucide;

use crate::{
    core::{OperationErrorCode, TransferDirection, TransferSnapshot, TransferStatus},
    ui::widgets::format_bytes,
};

/// One transfer: its direction, file, and progress or outcome. With
/// `show_device`, it names the other device, for lists that mix devices.
pub fn transfer_row<'a, M: 'a>(
    transfer: &'a TransferSnapshot,
    show_device: bool,
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
        details = details
            .push(container(progress_bar(0.0..=1.0, progress(transfer)).girth(4)).padding([4, 0]));
    }
    row![
        icon.size(18).style(text::secondary),
        container(details).width(Length::Fill).clip(true),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

/// How far along the bar is: the share sent while transferring, and empty
/// before that.
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
    use uuid::Uuid;

    use super::*;

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
}
