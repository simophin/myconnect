use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;
use uuid::Uuid;

use super::{create_private_dir, private_file_options};

/// User preferences as persisted in `settings.json`. A missing field means
/// "use the default", so defaults can change without rewriting the file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StoredSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_dir: Option<PathBuf>,
    /// Owned by the UI; the daemon stores it without interpreting it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_to_tray: Option<bool>,
    /// Plugins' settings sections, keyed by plugin id. Each holds only the
    /// fields the user set.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub plugins: BTreeMap<String, Map<String, Value>>,
    /// Top-level keys this build doesn't know, such as a setting that has
    /// since moved into a plugin's section. Read but never written back.
    #[serde(flatten, skip_serializing)]
    pub unrecognized: Map<String, Value>,
}

/// `settings.json` under the configuration directory.
#[derive(Clone, Debug)]
pub struct SettingsFile {
    directory: PathBuf,
}

impl SettingsFile {
    const FILE_NAME: &str = "settings.json";

    pub fn new(config_dir: impl AsRef<Path>) -> Self {
        Self {
            directory: config_dir.as_ref().to_path_buf(),
        }
    }

    /// Read the stored settings; a missing file means nothing is stored yet.
    pub fn load(&self) -> Result<StoredSettings, SettingsError> {
        match fs::read(self.directory.join(Self::FILE_NAME)) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| SettingsError::Corrupt),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(StoredSettings::default())
            }
            Err(error) => Err(SettingsError::Io(error)),
        }
    }

    /// Replace the stored settings atomically: write a temporary file, sync
    /// it, then rename it over the old one.
    pub fn save(&self, settings: &StoredSettings) -> Result<(), SettingsError> {
        create_private_dir(&self.directory).map_err(SettingsError::Io)?;
        let temporary = self
            .directory
            .join(format!(".settings-{}.tmp", Uuid::new_v4().simple()));
        let bytes = serde_json::to_vec_pretty(settings).map_err(|_| SettingsError::Encoding)?;

        let result = (|| {
            let mut file = private_file_options()
                .open(&temporary)
                .map_err(SettingsError::Io)?;
            file.write_all(&bytes).map_err(SettingsError::Io)?;
            file.sync_all().map_err(SettingsError::Io)?;
            fs::rename(&temporary, self.directory.join(Self::FILE_NAME))
                .map_err(SettingsError::Io)?;
            sync_directory(&self.directory).map_err(SettingsError::Io)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> std::io::Result<()> {
    fs::File::open(directory)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> std::io::Result<()> {
    Ok(())
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("settings storage operation failed")]
    Io(#[source] std::io::Error),
    #[error("settings file is corrupt")]
    Corrupt,
    #[error("settings could not be encoded")]
    Encoding,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_loads_as_empty_and_saved_settings_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let file = SettingsFile::new(directory.path().join("config"));
        assert_eq!(file.load().unwrap(), StoredSettings::default());

        let settings = StoredSettings {
            device_name: Some("Desk".into()),
            plugins: BTreeMap::from([(
                "wave".into(),
                Map::from_iter([("enabled".into(), Value::Bool(false))]),
            )]),
            ..Default::default()
        };
        file.save(&settings).unwrap();
        assert_eq!(file.load().unwrap(), settings);

        let raw = fs::read_to_string(directory.path().join("config/settings.json")).unwrap();
        assert!(!raw.contains("downloadDir"), "unset fields are omitted");
    }

    #[test]
    fn unknown_fields_are_read_but_not_saved_and_garbage_is_corrupt() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let file = SettingsFile::new(directory.path());

        fs::write(&path, r#"{"deviceName":"Desk","fromTheFuture":1}"#).unwrap();
        let loaded = file.load().unwrap();
        assert_eq!(loaded.device_name.as_deref(), Some("Desk"));
        assert_eq!(loaded.unrecognized["fromTheFuture"], 1);
        file.save(&loaded).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\n  \"deviceName\": \"Desk\"\n}"
        );

        fs::write(&path, "not json").unwrap();
        assert!(matches!(file.load(), Err(SettingsError::Corrupt)));
    }
}
