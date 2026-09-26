//! Where the packages put the notices for the third-party code in the app:
//! `THIRD_PARTY_LICENSES.html`, which cargo-about writes (`about.toml`) and
//! About opens. A build that wasn't packaged (`cargo run`) has none.

use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
};

const FILE_NAME: &str = "THIRD_PARTY_LICENSES.html";

/// The running app's notices.
pub fn path() -> &'static Path {
    static PATH: LazyLock<PathBuf> =
        LazyLock::new(|| beside(&std::env::current_exe().unwrap_or_default()));
    &PATH
}

/// The notices of the app installed as `exe`: in the bundle's `Resources` on
/// macOS, next to `Ferry.exe` on Windows, and in `/usr/share/doc/ferry` for
/// `/usr/bin/ferry-gui` on Linux.
fn beside(exe: &Path) -> PathBuf {
    let dir = exe.parent().unwrap_or(Path::new(""));
    let up = dir.parent().unwrap_or(dir);
    if cfg!(target_os = "macos") {
        up.join("Resources").join(FILE_NAME)
    } else if cfg!(windows) {
        dir.join(FILE_NAME)
    } else {
        up.join("share/doc/ferry").join(FILE_NAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notices_are_where_the_package_puts_them() {
        let (exe, notices) = if cfg!(target_os = "macos") {
            (
                "/Applications/Ferry.app/Contents/MacOS/Ferry",
                "/Applications/Ferry.app/Contents/Resources/THIRD_PARTY_LICENSES.html",
            )
        } else if cfg!(windows) {
            (
                r"C:\Users\me\AppData\Local\Programs\Ferry\Ferry.exe",
                r"C:\Users\me\AppData\Local\Programs\Ferry\THIRD_PARTY_LICENSES.html",
            )
        } else {
            (
                "/usr/bin/ferry-gui",
                "/usr/share/doc/ferry/THIRD_PARTY_LICENSES.html",
            )
        };
        assert_eq!(beside(Path::new(exe)), Path::new(notices));
    }
}
