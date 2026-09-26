//! Errors in words the user reads.
//!
//! Messages are keyed by the error's code ([`CoreError::code`]), the same
//! code the HTTP API reports, so the UI and the CLI name a failure the same
//! way. A feature with its own error type words its own codes and hands the
//! rest to [`describe_code`].

use std::path::{Path, PathBuf};

use crate::core::CoreError;

/// A sentence for the user about `error`.
pub fn describe_error(error: &CoreError) -> String {
    describe_code(error.code())
}

/// A sentence for the user about a failure with this code.
pub fn describe_code(code: &str) -> String {
    let message = match code {
        "daemon_unavailable" => "Ferry is not responding.",
        "unauthorized" => "Ferry rejected this app’s access token.",
        "device_not_found" => "That device is no longer known.",
        "device_not_connected" => "The device is not connected right now.",
        "already_paired" => "The device is already paired.",
        "pairing_in_progress" => "A pairing with this device is already running.",
        "pairing_not_found" => "That pairing request no longer exists.",
        "invalid_pairing_state" | "invalid_pairing_direction" => {
            "That pairing request is no longer active."
        }
        "device_not_paired" => "The device is not paired.",
        "unsupported_by_peer" => "The device doesn’t support that.",
        "invalid_file_name" => "That file name can’t be sent.",
        "transfer_too_large" | "payload_too_large" => "The file is too large to send.",
        "transfer_not_found" => "That transfer no longer exists.",
        "invalid_transfer_state" => "That transfer has already finished.",
        "request_timeout" => "Ferry took too long to respond.",
        "invalid_device_name" => {
            "Use 1 to 32 characters, without . , : ; ! ? ( ) [ ] < > or quotes."
        }
        "invalid_download_dir" => "That folder can’t be used for downloads.",
        "invalid_address" => "Enter an IPv4 address, like 192.168.1.20.",
        code => return format!("Something went wrong ({code})."),
    };
    message.into()
}

/// One sentence about the files among a batch that failed, if any:
/// `verb` is what was being done ("send", "upload"), and each failure is a
/// file and why. Only the first reason is given, as the Flutter app did.
pub fn describe_file_failures(verb: &str, failures: &[(PathBuf, String)]) -> Option<String> {
    match failures {
        [] => None,
        [(path, reason)] => Some(format!("Couldn’t {verb} {}: {reason}", file_name(path))),
        [(_, reason), ..] => Some(format!(
            "Couldn’t {verb} {} files: {reason}",
            failures.len()
        )),
    }
}

/// The last part of `path`, for the user.
pub fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every code the Flutter app worded that isn't a plugin's, worded
    /// the same.
    #[test]
    fn codes_read_like_the_flutter_app() {
        for (code, message) in [
            ("daemon_unavailable", "Ferry is not responding."),
            ("unauthorized", "Ferry rejected this app’s access token."),
            ("device_not_found", "That device is no longer known."),
            (
                "device_not_connected",
                "The device is not connected right now.",
            ),
            ("already_paired", "The device is already paired."),
            (
                "pairing_in_progress",
                "A pairing with this device is already running.",
            ),
            (
                "pairing_not_found",
                "That pairing request no longer exists.",
            ),
            (
                "invalid_pairing_state",
                "That pairing request is no longer active.",
            ),
            (
                "invalid_pairing_direction",
                "That pairing request is no longer active.",
            ),
            ("device_not_paired", "The device is not paired."),
            ("unsupported_by_peer", "The device doesn’t support that."),
            ("invalid_file_name", "That file name can’t be sent."),
            ("transfer_too_large", "The file is too large to send."),
            ("payload_too_large", "The file is too large to send."),
            ("transfer_not_found", "That transfer no longer exists."),
            (
                "invalid_transfer_state",
                "That transfer has already finished.",
            ),
            ("request_timeout", "Ferry took too long to respond."),
            (
                "invalid_device_name",
                "Use 1 to 32 characters, without . , : ; ! ? ( ) [ ] < > or quotes.",
            ),
            (
                "invalid_download_dir",
                "That folder can’t be used for downloads.",
            ),
            (
                "invalid_address",
                "Enter an IPv4 address, like 192.168.1.20.",
            ),
            ("teapot", "Something went wrong (teapot)."),
        ] {
            assert_eq!(describe_code(code), message, "{code}");
        }
    }

    #[test]
    fn core_errors_are_worded_by_their_code() {
        assert_eq!(
            describe_error(&CoreError::DeviceNotConnected),
            "The device is not connected right now."
        );
        assert_eq!(
            describe_error(&CoreError::InvalidDiscoveryAddress),
            "Enter an IPv4 address, like 192.168.1.20."
        );
        assert_eq!(
            describe_error(&CoreError::Internal),
            "Something went wrong (internal_error)."
        );
    }

    #[test]
    fn failed_files_are_summed_up_in_one_sentence() {
        let failed =
            |name: &str, reason: &str| (PathBuf::from(format!("/tmp/{name}")), reason.into());
        assert_eq!(describe_file_failures("send", &[]), None);
        assert_eq!(
            describe_file_failures("send", &[failed("photo.jpg", "It broke.")]).unwrap(),
            "Couldn’t send photo.jpg: It broke."
        );
        assert_eq!(
            describe_file_failures(
                "upload",
                &[failed("a.txt", "First."), failed("b.txt", "Second.")]
            )
            .unwrap(),
            "Couldn’t upload 2 files: First."
        );
    }
}
