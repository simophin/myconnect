//! Browse's errors, in words for the user.

use crate::{
    plugins::browse::{BrowseError, UploadPathError},
    ui::{error::describe_code, i18n::fl},
};

/// A sentence for the user about why an upload didn't start.
pub(super) fn describe_upload_error(error: &UploadPathError) -> String {
    match error {
        UploadPathError::Browse(error) => describe_error(error),
        UploadPathError::File(_) => fl!("browse-error-file-unreadable"),
    }
}

/// A sentence for the user about `error`.
pub fn describe_error(error: &BrowseError) -> String {
    let reason = match error {
        BrowseError::Unavailable { reason } => reason.as_deref(),
        _ => None,
    };
    describe(error.code(), reason)
}

/// Words this feature's codes, with the peer's `reason` where it gives one,
/// and leaves the rest to `ui::error`.
fn describe(code: &str, reason: Option<&str>) -> String {
    match code {
        "files_unavailable" => match reason {
            Some(reason) => fl!("browse-error-files_unavailable-reason", reason = reason),
            None => fl!("browse-error-files_unavailable"),
        },
        "file_not_found" => fl!("browse-error-file_not_found"),
        "file_exists" => fl!("browse-error-file_exists"),
        "file_permission_denied" => fl!("browse-error-file_permission_denied"),
        "not_a_directory" => fl!("browse-error-not_a_directory"),
        "is_a_directory" => fl!("browse-error-is_a_directory"),
        "invalid_path" => fl!("browse-error-invalid_path"),
        "files_failed" => fl!("browse-error-files_failed"),
        "files_timed_out" => fl!("browse-error-files_timed_out"),
        "files_host_key_mismatch" => fl!("browse-error-files_host_key_mismatch"),
        code => describe_code(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CoreError;

    /// The browse codes, worded as the Flutter app worded them.
    #[test]
    fn codes_read_like_the_flutter_app() {
        for (code, message) in [
            ("file_not_found", "That file or folder no longer exists."),
            (
                "file_exists",
                "There is already a file or folder with that name.",
            ),
            ("file_permission_denied", "The device doesn’t allow that."),
            ("not_a_directory", "That isn’t a folder."),
            ("is_a_directory", "Folders can’t be downloaded, only files."),
            ("invalid_path", "That name or location can’t be used."),
            ("files_failed", "The device’s files couldn’t be reached."),
            ("files_timed_out", "The device took too long to answer."),
            (
                "files_host_key_mismatch",
                "The device’s file server didn’t prove it is the paired device, so \
                 Ferry didn’t connect to it.",
            ),
        ] {
            assert_eq!(describe(code, None), message, "{code}");
        }
    }

    #[test]
    fn unavailable_says_why_when_the_device_does() {
        assert_eq!(
            describe_error(&BrowseError::Unavailable {
                reason: Some("No storage access".into())
            }),
            "The device isn’t sharing its files (No storage access). In KDE Connect on \
             the device, allow access to files in the Filesystem expose plugin."
        );
        assert_eq!(
            describe_error(&BrowseError::Unavailable { reason: None }),
            "The device isn’t sharing its files. In KDE Connect on the device, allow \
             access to files in the Filesystem expose plugin."
        );
    }

    #[test]
    fn core_errors_read_as_the_core_words_them() {
        assert_eq!(
            describe_error(&BrowseError::Core(CoreError::DeviceNotConnected)),
            "The device is not connected right now."
        );
    }
}
