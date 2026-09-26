use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{ApiToken, write_private_file};

/// The app's HTTP API as persisted in `api.json`: whether it serves it, on
/// which port, and the token clients must present. The token is kept even
/// while the API is off, so turning it back on doesn't break a CLI set up
/// with it. `ferry-cli` reads it too: on the same machine it needs no
/// token or port of its own.
///
/// Apart from `settings.json` because it holds a secret, which must not
/// reach `GET /settings` or the settings events.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StoredApi {
    pub enabled: bool,
    /// `None` means the default port.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) token: Option<String>,
}

impl StoredApi {
    /// The stored token, if there is a valid one.
    pub fn token(&self) -> Option<ApiToken> {
        self.token
            .as_deref()
            .and_then(|secret| ApiToken::from_secret(secret).ok())
    }

    pub fn set_token(&mut self, token: &ApiToken) {
        self.token = Some(token.expose_secret().to_owned());
    }
}

impl fmt::Debug for StoredApi {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredApi")
            .field("enabled", &self.enabled)
            .field("port", &self.port)
            .field("token", &self.token())
            .finish()
    }
}

/// `api.json` under the configuration directory, readable only by this
/// user.
#[derive(Clone, Debug)]
pub struct ApiFile {
    directory: PathBuf,
}

impl ApiFile {
    const FILE_NAME: &str = "api.json";

    pub fn new(config_dir: impl AsRef<Path>) -> Self {
        Self {
            directory: config_dir.as_ref().to_path_buf(),
        }
    }

    /// Read what is stored; a missing file means the API was never turned
    /// on.
    pub fn load(&self) -> Result<StoredApi, ApiFileError> {
        match fs::read(self.directory.join(Self::FILE_NAME)) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| ApiFileError::Corrupt),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(StoredApi::default()),
            Err(error) => Err(ApiFileError::Io(error)),
        }
    }

    pub fn save(&self, api: &StoredApi) -> Result<(), ApiFileError> {
        let bytes = serde_json::to_vec_pretty(api).map_err(|_| ApiFileError::Encoding)?;
        write_private_file(&self.directory, Self::FILE_NAME, &bytes).map_err(ApiFileError::Io)
    }
}

#[derive(Debug, Error)]
pub enum ApiFileError {
    #[error("API settings storage operation failed")]
    Io(#[source] std::io::Error),
    #[error("API settings file is corrupt")]
    Corrupt,
    #[error("API settings could not be encoded")]
    Encoding,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_off_and_saved_settings_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let file = ApiFile::new(directory.path().join("config"));
        let empty = file.load().unwrap();
        assert!(!empty.enabled);
        assert_eq!(empty.token(), None);

        let token = ApiToken::generate();
        let mut api = StoredApi {
            enabled: true,
            port: Some(25_000),
            ..StoredApi::default()
        };
        api.set_token(&token);
        file.save(&api).unwrap();
        let loaded = file.load().unwrap();
        assert_eq!(loaded, api);
        assert_eq!(loaded.token(), Some(token.clone()));
        assert!(!format!("{loaded:?}").contains(token.expose_secret()));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(directory.path().join("config/api.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn an_invalid_token_reads_as_none_and_garbage_is_corrupt() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("api.json");
        let file = ApiFile::new(directory.path());

        fs::write(&path, r#"{"enabled":true,"token":"has space"}"#).unwrap();
        let loaded = file.load().unwrap();
        assert!(loaded.enabled);
        assert_eq!(loaded.token(), None);

        fs::write(&path, "not json").unwrap();
        assert!(matches!(file.load(), Err(ApiFileError::Corrupt)));
    }
}
