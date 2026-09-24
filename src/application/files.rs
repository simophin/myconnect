//! API-facing types for browsing a peer's files, and the pure path checks
//! applied to every remote path a client names. Kept free of sockets so the
//! checks can be tested in isolation.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// What a directory entry is. Symbolic links are reported as such, not
/// followed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    File,
    Directory,
    Symlink,
    Other,
}

/// One file or directory on a peer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    /// Absolute path on the peer, `/`-separated.
    pub path: String,
    pub kind: FileKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Last modification, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<u64>,
}

/// The contents of one directory on a peer, or, with no `path`, the storage
/// roots the peer exposes (as directories named by the peer).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryListing {
    pub path: Option<String>,
    pub entries: Vec<FileEntry>,
}

/// Check that `path` is a plain absolute path on the peer and return it in
/// canonical form: no trailing or repeated `/`, and no `.` or `..`
/// segments, so a path can't mean something other than what it spells.
pub fn normalize_remote_path(path: &str) -> Result<String, RemotePathError> {
    if !path.starts_with('/') || path.contains('\0') {
        return Err(RemotePathError);
    }
    let mut normalized = String::with_capacity(path.len());
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        if segment == "." || segment == ".." {
            return Err(RemotePathError);
        }
        normalized.push('/');
        normalized.push_str(segment);
    }
    if normalized.is_empty() {
        normalized.push('/');
    }
    Ok(normalized)
}

/// Split a normalized path into its parent directory and final name. `/`
/// has no name.
pub fn split_remote_path(path: &str) -> Option<(&str, &str)> {
    let (parent, name) = path.rsplit_once('/')?;
    if name.is_empty() {
        return None;
    }
    Some((if parent.is_empty() { "/" } else { parent }, name))
}

/// Join a normalized directory path and a single name.
pub fn join_remote_path(directory: &str, name: &str) -> String {
    if directory.ends_with('/') {
        format!("{directory}{name}")
    } else {
        format!("{directory}/{name}")
    }
}

/// Check that `name` is a single path segment usable as a file name.
pub fn validate_remote_name(name: &str) -> Result<(), RemotePathError> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']) {
        return Err(RemotePathError);
    }
    Ok(())
}

/// The name to try on the `attempt`th collision: `photo (1).jpg`,
/// `photo (2).jpg`, and so on, matching how downloads are named locally.
pub fn numbered_name(name: &str, attempt: u32) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => format!("{stem} ({attempt}).{extension}"),
        _ => format!("{name} ({attempt})"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("remote path must be absolute, without `.` or `..` segments")]
pub struct RemotePathError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_normalized() {
        assert_eq!(normalize_remote_path("/").unwrap(), "/");
        assert_eq!(normalize_remote_path("//").unwrap(), "/");
        assert_eq!(
            normalize_remote_path("/storage//emulated/0/").unwrap(),
            "/storage/emulated/0"
        );
        assert_eq!(
            normalize_remote_path("/a/b c/d.txt").unwrap(),
            "/a/b c/d.txt"
        );
    }

    #[test]
    fn relative_and_traversing_paths_are_rejected() {
        for path in ["", "relative", "/a/../b", "/a/./b", "/..", "/a\0b"] {
            assert_eq!(normalize_remote_path(path), Err(RemotePathError), "{path}");
        }
    }

    #[test]
    fn paths_split_into_parent_and_name() {
        assert_eq!(split_remote_path("/a/b.txt"), Some(("/a", "b.txt")));
        assert_eq!(split_remote_path("/a"), Some(("/", "a")));
        assert_eq!(split_remote_path("/"), None);
        assert_eq!(join_remote_path("/", "a"), "/a");
        assert_eq!(join_remote_path("/a", "b"), "/a/b");
    }

    #[test]
    fn names_are_single_segments() {
        assert!(validate_remote_name("photo.jpg").is_ok());
        assert!(validate_remote_name(".hidden").is_ok());
        for name in ["", ".", "..", "a/b", "a\0b"] {
            assert_eq!(validate_remote_name(name), Err(RemotePathError), "{name}");
        }
    }

    #[test]
    fn collisions_are_numbered_before_the_extension() {
        assert_eq!(numbered_name("photo.jpg", 1), "photo (1).jpg");
        assert_eq!(numbered_name("archive.tar.gz", 2), "archive.tar (2).gz");
        assert_eq!(numbered_name("README", 1), "README (1)");
        assert_eq!(numbered_name(".bashrc", 1), ".bashrc (1)");
    }
}
