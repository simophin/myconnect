//! On macOS the app lives in the menu bar: it has a Dock icon (and a place
//! in the app switcher) only while its window is open. With the window
//! closed, a Dock icon would do nothing when clicked, as winit doesn't
//! handle `applicationShouldHandleReopen:`. Elsewhere this does nothing.

/// Show the app in the Dock, activating it, or take it out. Only takes on
/// the main thread, where `update` runs.
pub fn show(shown: bool) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

        let Some(main_thread) = MainThreadMarker::new() else {
            return;
        };
        let app = NSApplication::sharedApplication(main_thread);
        let policy = if shown {
            NSApplicationActivationPolicy::Regular
        } else {
            NSApplicationActivationPolicy::Accessory
        };
        if !app.setActivationPolicy(policy) {
            tracing::warn!(shown, "the Dock icon wasn't changed");
        }
        if shown {
            // `activate` needs macOS 14.
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = shown;
}
