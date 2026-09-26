//! Persistent local identity and user settings, and the API token.

mod identity;
mod settings;
mod token;

use std::path::PathBuf;

use directories::ProjectDirs;

pub use identity::{IdentityError, LocalIdentity};
pub use settings::{SettingsError, SettingsFile, StoredSettings};
pub use token::{ApiToken, ApiTokenError};

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

pub(crate) fn create_private_dir(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }

    Ok(())
}
