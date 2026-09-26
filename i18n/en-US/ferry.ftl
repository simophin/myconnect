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
