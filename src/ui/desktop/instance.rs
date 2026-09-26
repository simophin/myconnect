//! One app per data directory: a second launch asks the running one to
//! show its window, and exits.
//!
//! A local socket named from the user and the data dir (ADR 0001), so
//! isolated test instances never collide with the owner's app: an abstract
//! socket on Linux (nothing left behind by a crash), a file under `/tmp` on
//! macOS (replaced once a connect is refused), a named pipe on Windows.

use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    thread,
};

use interprocess::local_socket::{
    GenericNamespaced, ListenerOptions, Stream, prelude::*, traits::ListenerExt,
};
use sha2::{Digest, Sha256};

use super::{DesktopEvent, Events};

/// What a second launch sends.
const SHOW: &str = "show";

/// Whether this launch is the one that runs.
pub enum Instance {
    /// This one: the listener is running, sending
    /// [`DesktopEvent::ShowRequested`] for each later launch.
    First,
    /// Another one runs, and was asked to show its window.
    Running,
}

/// Become the instance for `data_dir`, or tell the one that is to show
/// itself. If neither works, this launch runs on its own.
pub fn claim(data_dir: &Path, events: Events) -> Instance {
    let name = name(data_dir);
    if let Ok(mut running) = connect(&name) {
        match writeln!(running, "{SHOW}") {
            Ok(()) => return Instance::Running,
            Err(error) => tracing::warn!(%error, "the running instance didn't answer"),
        }
    }
    match listen(&name, events) {
        Ok(()) => {}
        Err(error) => tracing::warn!(%error, "a second launch won't find this one"),
    }
    Instance::First
}

fn connect(name: &str) -> std::io::Result<Stream> {
    Stream::connect(name.to_ns_name::<GenericNamespaced>()?)
}

fn listen(name: &str, events: Events) -> std::io::Result<()> {
    let listener = ListenerOptions::new()
        .name(name.to_ns_name::<GenericNamespaced>()?)
        // A file left by a crash (macOS); a live one answered above.
        .try_overwrite(true)
        .create_sync()?;
    thread::Builder::new()
        .name("single-instance".into())
        .spawn(move || {
            for connection in listener.incoming() {
                let Ok(connection) = connection else {
                    continue;
                };
                let mut line = String::new();
                if BufReader::new(connection).read_line(&mut line).is_ok()
                    && line.trim() == SHOW
                    && events.send(DesktopEvent::ShowRequested).is_err()
                {
                    return;
                }
            }
        })?;
    Ok(())
}

/// `ferry-<user>-<hash of the data dir>`: short enough for macOS's
/// socket paths, and the user keeps users apart in Linux's shared abstract
/// namespace. The directory is made absolute but not canonical: it may not
/// exist yet on a first launch, and a second must get the same name.
fn name(data_dir: &Path) -> String {
    let absolute = std::path::absolute(data_dir).unwrap_or_else(|_| data_dir.to_owned());
    let hash = Sha256::digest(absolute.to_string_lossy().as_bytes());
    let hash: String = hash[..8].iter().map(|byte| format!("{byte:02x}")).collect();
    format!("ferry-{}-{hash}", user())
}

#[cfg(unix)]
fn user() -> String {
    // SAFETY: getuid has no preconditions and can't fail.
    unsafe { libc::getuid() }.to_string()
}

#[cfg(not(unix))]
fn user() -> String {
    std::env::var("USERNAME").unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_launch_shows_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
        assert!(matches!(claim(dir.path(), events.clone()), Instance::First));
        assert!(matches!(claim(dir.path(), events), Instance::Running));
        let shown = received.blocking_recv();
        assert!(matches!(shown, Some(DesktopEvent::ShowRequested)));

        // Another data dir is another app.
        let other = tempfile::tempdir().unwrap();
        let (events, _received) = tokio::sync::mpsc::unbounded_channel();
        assert!(matches!(claim(other.path(), events), Instance::First));
    }

    #[test]
    fn a_data_dir_made_after_the_first_launch_keeps_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let before = name(&data);
        std::fs::create_dir(&data).unwrap();
        assert_eq!(name(&data), before);
    }

    #[test]
    fn names_differ_by_data_dir() {
        let a = name(Path::new("/tmp/a"));
        assert_eq!(a, name(Path::new("/tmp/a")));
        assert_ne!(a, name(Path::new("/tmp/b")));
        assert!(a.starts_with("ferry-") && a.len() < 50, "{a}");
    }
}
