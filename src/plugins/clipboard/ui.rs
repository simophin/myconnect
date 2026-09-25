//! The clipboard feature's UI half.

use super::ClipboardSyncError;
use crate::ui::error::describe_code;

/// A sentence for the user about `error`.
pub fn describe_error(error: &ClipboardSyncError) -> String {
    describe(error.code())
}

/// Words this feature's codes and leaves the rest to the UI core.
fn describe(code: &str) -> String {
    match code {
        "clipboard_empty" => "There is no text on the clipboard to send.".into(),
        "clipboard_text_too_large" => "The clipboard text is too long to send.".into(),
        code => describe_code(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CoreError;

    /// The clipboard codes `ui/lib/src/core/api/api_exception.dart` words.
    #[test]
    fn codes_read_like_the_flutter_app() {
        assert_eq!(
            describe_error(&ClipboardSyncError::Empty),
            "There is no text on the clipboard to send."
        );
        assert_eq!(
            describe_error(&ClipboardSyncError::TextTooLarge { limit: 1 }),
            "The clipboard text is too long to send."
        );
        assert_eq!(
            describe_error(&ClipboardSyncError::Core(CoreError::UnsupportedByPeer)),
            "The device doesn’t support that."
        );
    }
}
