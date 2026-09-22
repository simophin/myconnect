//! Pure helpers for file transfer resource management: destination filename
//! sanitization, collision-avoiding naming, and transfer-subsystem
//! configuration. Kept free of sockets and application state so path
//! sanitization can be tested in isolation.

use std::{
    net::Ipv4Addr,
    path::{Path, PathBuf},
    time::Duration,
};

use thiserror::Error;

/// Conservative default cap on a single incoming or outgoing transfer, in
/// bytes. This exists to keep a misbehaving or malicious peer from causing
/// unbounded disk use; it is not a KDE Connect protocol limit.
pub const DEFAULT_MAX_TRANSFER_BYTES: u64 = 10 * 1024 * 1024 * 1024;

/// Default deadline for establishing an auxiliary payload connection.
pub const DEFAULT_PAYLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Configuration for the transfer subsystem.
#[derive(Clone, Debug)]
pub struct TransferConfig {
    pub download_dir: PathBuf,
    pub max_transfer_bytes: u64,
    pub payload_bind_ip: Ipv4Addr,
    pub payload_connect_timeout: Duration,
}

impl TransferConfig {
    pub fn new(download_dir: PathBuf) -> Self {
        Self {
            download_dir,
            max_transfer_bytes: DEFAULT_MAX_TRANSFER_BYTES,
            payload_bind_ip: Ipv4Addr::UNSPECIFIED,
            payload_connect_timeout: DEFAULT_PAYLOAD_CONNECT_TIMEOUT,
        }
    }

    pub fn with_max_transfer_bytes(mut self, value: u64) -> Self {
        self.max_transfer_bytes = value;
        self
    }

    pub fn with_payload_bind_ip(mut self, ip: Ipv4Addr) -> Self {
        self.payload_bind_ip = ip;
        self
    }

    pub fn with_payload_connect_timeout(mut self, value: Duration) -> Self {
        self.payload_connect_timeout = value;
        self
    }
}

/// Reduce a peer-declared filename to a single safe path component: strip
/// any directory parts, then reject anything empty, `.`, `..`, or containing
/// a NUL byte. This is the sole path-traversal defense for incoming
/// transfers: the sanitized name is later joined to the configured download
/// directory and never otherwise interpreted as a path.
pub fn sanitize_file_name(name: &str) -> Result<String, FileNameError> {
    if name.contains('\0') {
        return Err(FileNameError::Invalid);
    }
    let base = Path::new(name)
        .file_name()
        .ok_or(FileNameError::Invalid)?
        .to_str()
        .ok_or(FileNameError::Invalid)?;
    if base.is_empty() || base == "." || base == ".." {
        return Err(FileNameError::Invalid);
    }
    Ok(base.to_owned())
}

/// Return a destination path under `dir` for `file_name`, appending a
/// ` (n)` suffix before the extension if the name already exists, so a
/// completed transfer never silently overwrites an unrelated file.
pub fn unique_destination(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(file_name);
    let extension = path.extension().and_then(|value| value.to_str());
    for attempt in 1_u32.. {
        let candidate_name = match extension {
            Some(extension) => format!("{stem} ({attempt}).{extension}"),
            None => format!("{stem} ({attempt})"),
        };
        let candidate = dir.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u32 attempts exhausted")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum FileNameError {
    #[error("file name is empty, absolute, or a path traversal attempt")]
    Invalid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_directory_components() {
        assert_eq!(sanitize_file_name("photo.jpg").unwrap(), "photo.jpg");
        assert_eq!(sanitize_file_name("../../etc/passwd").unwrap(), "passwd");
        assert_eq!(sanitize_file_name("/etc/shadow").unwrap(), "shadow");
        assert_eq!(
            sanitize_file_name("a/b/c/report.pdf").unwrap(),
            "report.pdf"
        );
    }

    #[test]
    fn sanitize_rejects_degenerate_names() {
        assert_eq!(sanitize_file_name(""), Err(FileNameError::Invalid));
        assert_eq!(sanitize_file_name("."), Err(FileNameError::Invalid));
        assert_eq!(sanitize_file_name(".."), Err(FileNameError::Invalid));
        assert_eq!(sanitize_file_name("../.."), Err(FileNameError::Invalid));
        assert_eq!(sanitize_file_name("a/.."), Err(FileNameError::Invalid));
        assert_eq!(sanitize_file_name("bad\0name"), Err(FileNameError::Invalid));
    }

    #[test]
    fn unique_destination_avoids_overwriting_existing_files() {
        let directory = tempfile::tempdir().unwrap();
        let first = unique_destination(directory.path(), "photo.jpg");
        assert_eq!(first, directory.path().join("photo.jpg"));
        std::fs::write(&first, b"existing").unwrap();

        let second = unique_destination(directory.path(), "photo.jpg");
        assert_eq!(second, directory.path().join("photo (1).jpg"));
        std::fs::write(&second, b"existing").unwrap();

        let third = unique_destination(directory.path(), "photo.jpg");
        assert_eq!(third, directory.path().join("photo (2).jpg"));
    }
}
