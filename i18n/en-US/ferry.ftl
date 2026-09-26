# Ferry's desktop app, in US English: the source of every other language,
# and the fallback for any message a translation lacks. `fl!` checks keys
# and arguments against this file at compile time.
#
# Keys are prefixed by feature (`browse-…`, `clipboard-…`, `error-<code>`),
# grouped under a header per feature. Whole sentences only; names, paths
# and numbers are arguments; every count goes through a plural selector.
# See docs/PLAN_I18N.md.

## Files dropped on the window or the tray icon (src/ui/drops.rs)

# The header of the tray menu that asks where to send files dropped on the
# tray icon; the device names follow as menu items.
drop-send-to-header =
    { $count ->
        [one] Send { $count } file to:
       *[other] Send { $count } files to:
    }
drop-no-recipients = No paired device can receive files
# The title of a notification (or the start of a toast) when dropped files
# can't be sent; the reason is its body.
drop-send-failed = Couldn’t send
drop-folders-refused = Only files can be sent, not folders.

## Errors, keyed by the code the HTTP API reports (src/ui/error.rs)

error-daemon_unavailable = Ferry is not responding.
error-unauthorized = Ferry rejected this app’s access token.
error-device_not_found = That device is no longer known.
error-device_not_connected = The device is not connected right now.
error-already_paired = The device is already paired.
error-pairing_in_progress = A pairing with this device is already running.
error-pairing_not_found = That pairing request no longer exists.
# Also for `invalid_pairing_direction`.
error-invalid_pairing_state = That pairing request is no longer active.
error-device_not_paired = The device is not paired.
error-unsupported_by_peer = The device doesn’t support that.
error-invalid_file_name = That file name can’t be sent.
# Also for `payload_too_large`.
error-transfer_too_large = The file is too large to send.
error-transfer_not_found = That transfer no longer exists.
error-invalid_transfer_state = That transfer has already finished.
error-request_timeout = Ferry took too long to respond.
error-invalid_device_name = Use 1 to 32 characters, without . , : ; ! ? ( ) [ ] < > or quotes.
error-invalid_download_dir = That folder can’t be used for downloads.
error-invalid_address = Enter an IPv4 address, like 192.168.1.20.
# Any other code; `code` is the API's, such as `internal_error`.
error-unknown = Something went wrong ({ $code }).
# One of the files being sent failed; `reason` is a sentence saying why.
error-send-file-failed = Couldn’t send { $file }: { $reason }
# Several files being sent failed; `reason` is why the first one did.
error-send-files-failed =
    { $count ->
        [one] Couldn’t send { $count } file: { $reason }
       *[other] Couldn’t send { $count } files: { $reason }
    }
# The same, for files being uploaded to a device's folder.
error-upload-file-failed = Couldn’t upload { $file }: { $reason }
error-upload-files-failed =
    { $count ->
        [one] Couldn’t upload { $count } file: { $reason }
       *[other] Couldn’t upload { $count } files: { $reason }
    }

## Starting the app (src/ui/launch.rs, src/ui/pages/startup.rs)

# The window's title: the app's name.
app-window-title = Ferry
startup-starting = Starting Ferry…
# `error` is why, in English (it comes from the logs' wording).
startup-failed = Ferry could not start.
    { $error }

## The shell: the window's own messages and dialogs (src/ui/mod.rs,
## src/ui/shell.rs, src/ui/actions.rs)

# A notification shown as a toast while the window is focused.
shell-notification-toast = { $title }: { $body }
shell-unknown-device = This device is no longer known.
shell-unpair-title = Unpair device?
shell-unpair-body = { $name } will need to be paired again before it can exchange anything with this computer.
shell-unpair-confirm = Unpair
# `path` is the file's full path.
shell-open-failed = Couldn’t open { $path }
# `url` is a web address.
shell-open-link-failed = Couldn’t open { $url }
shell-start-on-login-failed = Couldn’t change starting on login.
shell-add-by-address-title = Add by IP address
shell-add-by-address-label = IP address
shell-add-by-address-helper = Ferry or KDE Connect must be running on that device. It appears in the list once it answers.
shell-add-by-address-confirm = Add
shell-rename-title = Device name
shell-rename-helper = How this computer appears on your other devices
shell-rename-confirm = Save
# The title of the folder picker for the download folder.
shell-download-dir-title = Save received files in

## Widgets the pages share (src/ui/widgets.rs)

widget-back = Back
widget-retry = Retry

## Browsing a device's files: errors (src/ui/features/browse/describe.rs)

browse-error-file-unreadable = The file couldn’t be read.
browse-error-files_unavailable = The device isn’t sharing its files. In KDE Connect on the device, allow access to files in the Filesystem expose plugin.
# `reason` is the device's own words, in its language.
browse-error-files_unavailable-reason = The device isn’t sharing its files ({ $reason }). In KDE Connect on the device, allow access to files in the Filesystem expose plugin.
browse-error-file_not_found = That file or folder no longer exists.
browse-error-file_exists = There is already a file or folder with that name.
browse-error-file_permission_denied = The device doesn’t allow that.
browse-error-not_a_directory = That isn’t a folder.
browse-error-is_a_directory = Folders can’t be downloaded, only files.
browse-error-invalid_path = That name or location can’t be used.
browse-error-files_failed = The device’s files couldn’t be reached.
browse-error-files_timed_out = The device took too long to answer.
browse-error-files_host_key_mismatch = The device’s file server didn’t prove it is the paired device, so Ferry didn’t connect to it.

## The clipboard: errors (src/ui/features/clipboard.rs)

clipboard-error-clipboard_empty = There is no text on the clipboard to send.
clipboard-error-clipboard_text_too_large = The clipboard text is too long to send.

## A device's notifications: why an action on one failed
## (src/ui/features/notifications.rs)

notifications-error-notification_not_found = It’s no longer on the device.
notifications-error-notification_not_repliable = It doesn’t take a reply.
notifications-error-notification_not_dismissable = It can’t be dismissed from here.
notifications-error-unknown_notification_action = It no longer has that button.
notifications-error-empty_reply = Write a message.

## Sending files: errors (src/ui/features/share.rs)

share-error-file-unreadable = The file couldn’t be read.
