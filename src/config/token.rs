use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use thiserror::Error;
use uuid::Uuid;

use super::{create_private_dir, private_file_options};

const TOKEN_FILE: &str = "api-token";
const TOKEN_LENGTH: usize = 64;

/// Persistent secret used to authenticate local API clients.
///
/// This type intentionally does not implement `Debug` or serialization.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiToken(String);

impl ApiToken {
    pub fn load_or_create(config_dir: impl AsRef<Path>) -> Result<Self, ApiTokenError> {
        let config_dir = config_dir.as_ref();
        let path = config_dir.join(TOKEN_FILE);
        match fs::read_to_string(&path) {
            Ok(value) => Self::parse(value),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                create_private_dir(config_dir).map_err(ApiTokenError::Io)?;
                let token = Self(format!(
                    "{}{}",
                    Uuid::new_v4().simple(),
                    Uuid::new_v4().simple()
                ));
                token.persist_atomically(&path)?;
                Ok(token)
            }
            Err(error) => Err(ApiTokenError::Io(error)),
        }
    }

    /// Reveal the token only at the boundary that constructs authentication.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    pub(crate) fn constant_time_matches(&self, candidate: &str) -> bool {
        if candidate.len() != self.0.len() {
            return false;
        }
        self.0
            .bytes()
            .zip(candidate.bytes())
            .fold(0_u8, |difference, (expected, actual)| {
                difference | (expected ^ actual)
            })
            == 0
    }

    fn parse(value: String) -> Result<Self, ApiTokenError> {
        if value.len() != TOKEN_LENGTH || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ApiTokenError::Corrupt);
        }
        Ok(Self(value))
    }

    fn persist_atomically(&self, destination: &Path) -> Result<(), ApiTokenError> {
        let temporary = temporary_path(destination);
        let result = (|| {
            let mut file = private_file_options()
                .open(&temporary)
                .map_err(ApiTokenError::Io)?;
            file.write_all(self.0.as_bytes())
                .map_err(ApiTokenError::Io)?;
            file.sync_all().map_err(ApiTokenError::Io)?;
            fs::rename(&temporary, destination).map_err(ApiTokenError::Io)?;
            sync_parent(destination).map_err(ApiTokenError::Io)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn temporary_path(destination: &Path) -> PathBuf {
    destination.with_file_name(format!(".api-token-{}.tmp", Uuid::new_v4().simple()))
}

#[cfg(unix)]
fn sync_parent(destination: &Path) -> std::io::Result<()> {
    fs::File::open(destination.parent().expect("token has a parent"))?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent(_destination: &Path) -> std::io::Result<()> {
    Ok(())
}

#[derive(Debug, Error)]
pub enum ApiTokenError {
    #[error("API token storage operation failed")]
    Io(#[source] std::io::Error),
    #[error("API token file is corrupt")]
    Corrupt,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_random_well_formed_and_persistent() {
        let directory = tempfile::tempdir().unwrap();
        let first = ApiToken::load_or_create(directory.path()).unwrap();
        let second = ApiToken::load_or_create(directory.path()).unwrap();
        assert_eq!(first.expose_secret(), second.expose_secret());
        assert_eq!(first.expose_secret().len(), TOKEN_LENGTH);
        assert!(
            first
                .expose_secret()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
        assert!(first.constant_time_matches(second.expose_secret()));
        assert!(!first.constant_time_matches("wrong"));
    }

    #[test]
    fn malformed_token_is_rejected_without_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(TOKEN_FILE);
        fs::write(&path, "unfinished").unwrap();
        assert!(matches!(
            ApiToken::load_or_create(directory.path()),
            Err(ApiTokenError::Corrupt)
        ));
        assert_eq!(fs::read_to_string(path).unwrap(), "unfinished");
    }

    #[cfg(unix)]
    #[test]
    fn token_file_has_private_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        ApiToken::load_or_create(directory.path()).unwrap();
        let mode = fs::metadata(directory.path().join(TOKEN_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
    }
}
