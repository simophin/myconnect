//! Opening files with their default app, showing them in the file
//! manager, and opening web pages in the browser.

use std::path::Path;

/// Opens files for the user. Calls may block briefly (a process spawn, a
/// D-Bus call), so the shell makes them off the UI thread.
pub trait Open: Send + Sync + 'static {
    /// Open `path` with its default app.
    fn open(&self, path: &Path) -> Result<(), String>;
    /// Show `path` in the file manager: selected where the platform can,
    /// otherwise its folder opened.
    fn reveal(&self, path: &Path) -> Result<(), String>;
    /// Open a web page in the default browser.
    fn browse(&self, url: &str) -> Result<(), String>;
}

/// The desktop's own handlers, through `opener`.
pub struct System;

impl Open for System {
    fn open(&self, path: &Path) -> Result<(), String> {
        // On Linux `xdg-open` is spawned and not waited for, so a missing
        // file would fail silently.
        exists(path)?;
        opener::open(path).map_err(|error| error.to_string())
    }

    fn reveal(&self, path: &Path) -> Result<(), String> {
        exists(path)?;
        opener::reveal(path).map_err(|error| error.to_string())
    }

    fn browse(&self, url: &str) -> Result<(), String> {
        opener::open_browser(url).map_err(|error| error.to_string())
    }
}

fn exists(path: &Path) -> Result<(), String> {
    match path.try_exists() {
        Ok(true) => Ok(()),
        Ok(false) => Err("no such file".into()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_refused_before_anything_opens() {
        let missing = Path::new("/nonexistent/ferry/test.txt");
        assert_eq!(System.open(missing), Err("no such file".into()));
        assert_eq!(System.reveal(missing), Err("no such file".into()));
    }
}
