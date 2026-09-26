//! Persistent local identity, peer trust, and user settings.

mod api;
mod identity;
mod settings;
mod token;
mod trust;

use std::path::PathBuf;

use directories::ProjectDirs;

pub use api::{ApiFile, ApiFileError, StoredApi};
pub use identity::{IdentityError, LocalIdentity};
pub use settings::{SettingsError, SettingsFile, StoredSettings};
pub use token::{ApiToken, ApiTokenError};
pub use trust::{FilesystemTrustStore, TrustError, TrustStore, TrustedDevice, TrustedIdentity};

/// Return the platform-specific directory used for Ferry configuration.
pub fn default_config_dir() -> Option<PathBuf> {
    ProjectDirs::from("dev", "fanchao", "Ferry")
        .map(|directories| directories.config_dir().to_path_buf())
}

pub(crate) fn is_valid_device_id(device_id: &str) -> bool {
    (32..=38).contains(&device_id.len())
        && device_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

#[cfg(unix)]
pub(crate) fn private_file_options() -> std::fs::OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    options
}

#[cfg(not(unix))]
pub(crate) fn private_file_options() -> std::fs::OpenOptions {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    options
}

/// Replace `directory/name` with `bytes` atomically, readable only by this
/// user: write a temporary file, sync it, then rename it over the old one.
pub(crate) fn write_private_file(
    directory: &std::path::Path,
    name: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    use std::io::Write;

    create_private_dir(directory)?;
    let temporary = directory.join(format!(".{name}-{}.tmp", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        let mut file = private_file_options().open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, directory.join(name))?;
        sync_directory(directory)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(unix)]
fn sync_directory(directory: &std::path::Path) -> std::io::Result<()> {
    std::fs::File::open(directory)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_directory: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

pub(crate) fn create_private_dir(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }

    Ok(())
}
