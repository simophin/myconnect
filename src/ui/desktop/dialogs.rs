//! The desktop's own file and folder pickers.

use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
};

/// What a picker answers: the choice, or `None` if it was cancelled.
pub type Picked<T> = Pin<Box<dyn Future<Output = Option<T>> + Send>>;

/// Asks the user to choose files or folders. The dialog shows while the
/// future runs; it needs no runtime of its own.
pub trait Pick: Send + Sync + 'static {
    /// Choose a folder, starting in `start`.
    fn pick_folder(&self, title: &str, start: &Path) -> Picked<PathBuf>;

    /// Choose one or more files to open.
    fn pick_files(&self, title: &str) -> Picked<Vec<PathBuf>>;
}

/// The platform's dialogs, through `rfd`: the XDG portal (or zenity) on
/// Linux, AppKit on macOS, the common dialogs on Windows.
pub struct System;

impl Pick for System {
    fn pick_folder(&self, title: &str, start: &Path) -> Picked<PathBuf> {
        let dialog = rfd::AsyncFileDialog::new()
            .set_title(title)
            .set_directory(start)
            .pick_folder();
        Box::pin(async move { dialog.await.map(|folder| folder.path().to_owned()) })
    }

    fn pick_files(&self, title: &str) -> Picked<Vec<PathBuf>> {
        let dialog = rfd::AsyncFileDialog::new().set_title(title).pick_files();
        Box::pin(async move {
            dialog
                .await
                .map(|files| files.iter().map(|file| file.path().to_owned()).collect())
        })
    }
}
