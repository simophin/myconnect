//! Errors in words the user reads.
//!
//! Messages are keyed by the error's code ([`CoreError::code`]), the same
//! code the HTTP API reports, so the UI and the CLI name a failure the same
//! way. A plugin with its own error type words its own codes and hands the
//! rest to [`describe_code`].

use crate::core::CoreError;

/// A sentence for the user about `error`.
pub fn describe_error(error: &CoreError) -> String {
    describe_code(error.code())
}

/// A sentence for the user about a failure with this code.
pub fn describe_code(code: &str) -> String {
    let message = match code {
        "daemon_unavailable" => "MyConnect is not responding.",
        "unauthorized" => "MyConnect rejected this app’s access token.",
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
        "request_timeout" => "MyConnect took too long to respond.",
        "invalid_device_name" => {
            "Use 1 to 32 characters, without . , : ; ! ? ( ) [ ] < > or quotes."
        }
        "invalid_download_dir" => "That folder can’t be used for downloads.",
        "invalid_address" => "Enter an IPv4 address, like 192.168.1.20.",
        code => return format!("Something went wrong ({code})."),
    };
    message.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every code `ui/lib/src/core/api/api_exception.dart` words that
    /// isn't a plugin's.
    #[test]
    fn codes_read_like_the_flutter_app() {
        for (code, message) in [
            ("daemon_unavailable", "MyConnect is not responding."),
            (
                "unauthorized",
                "MyConnect rejected this app’s access token.",
            ),
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
            ("request_timeout", "MyConnect took too long to respond."),
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
}
