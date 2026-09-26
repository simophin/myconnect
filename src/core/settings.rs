//! User settings: persisted preferences, layered under the start options of
//! the current run and over built-in defaults. Kept free of sockets and
//! the rest of the core's state so the precedence rules can be tested on their own.
//!
//! The core's own settings are typed fields. Each plugin with settings owns
//! a section under `plugins.<id>` (see [`super::PluginSettings`]); the core
//! stores, merges and publishes sections without knowing their fields.

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use super::{CoreError, plugin::SettingsSection};
use crate::{
    config::{SettingsFile, StoredSettings},
    protocol::is_valid_device_name,
};

/// What a setting falls back to when neither a start option nor the
/// settings file sets it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsDefaults {
    pub device_name: String,
    pub download_dir: PathBuf,
}

/// The settings in effect, as returned by `GET /settings`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub device_name: String,
    pub download_dir: PathBuf,
    /// Owned by the UI; the daemon stores it without interpreting it.
    pub close_to_tray: bool,
    /// Every plugin's settings section, keyed by plugin id, with defaults
    /// filled in.
    #[serde(default)]
    pub plugins: BTreeMap<String, Value>,
}

/// A partial update, as accepted by `PATCH /settings`. An absent field is
/// left alone; `null` resets it to its default. A plugin's section, under
/// `plugins`, is an object of the fields to change in the same way, or
/// `null` to reset the whole section.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    pub device_name: Option<Option<String>>,
    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    pub download_dir: Option<Option<PathBuf>>,
    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    pub close_to_tray: Option<Option<bool>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub plugins: BTreeMap<String, Value>,
}

impl SettingsPatch {
    /// A patch that changes `fields` of one plugin's section.
    pub fn plugin(id: &str, fields: Map<String, Value>) -> Self {
        Self {
            plugins: BTreeMap::from([(id.to_owned(), Value::Object(fields))]),
            ..Self::default()
        }
    }
}

/// Tells a field that is present but `null` (`Some(None)`) apart from one
/// that is absent (`None`, via `#[serde(default)]`).
fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Stored settings plus this run's start options.
///
/// A start option (a flag of `myconnect run` or the app) overrides the stored
/// value for the run it was given to, without being persisted. Changing a
/// setting through [`Settings::update`] persists it and drops the override,
/// since the user's latest choice should win.
#[derive(Debug)]
pub(crate) struct Settings {
    defaults: SettingsDefaults,
    stored: StoredSettings,
    overrides: StoredSettings,
    file: Option<SettingsFile>,
    sections: Vec<SettingsSection>,
}

impl Settings {
    /// Settings held only in memory, with nothing stored or overridden.
    pub(crate) fn new(defaults: SettingsDefaults) -> Self {
        Self {
            defaults,
            stored: StoredSettings::default(),
            overrides: StoredSettings::default(),
            file: None,
            sections: Vec::new(),
        }
    }

    /// Hold the plugins' sections.
    pub(crate) fn with_sections(mut self, sections: Vec<SettingsSection>) -> Self {
        self.sections = sections;
        self
    }

    /// Persist changes to `file`, starting from what it already holds.
    pub(crate) fn with_file(mut self, file: SettingsFile, stored: StoredSettings) -> Self {
        self.file = Some(file);
        self.stored = stored;
        self
    }

    pub(crate) fn with_overrides(mut self, overrides: StoredSettings) -> Self {
        self.overrides = overrides;
        self
    }

    pub(crate) fn snapshot(&self) -> SettingsSnapshot {
        let (stored, overrides) = (&self.stored, &self.overrides);
        SettingsSnapshot {
            device_name: overrides
                .device_name
                .clone()
                .or_else(|| stored.device_name.clone())
                .unwrap_or_else(|| self.defaults.device_name.clone()),
            download_dir: overrides
                .download_dir
                .clone()
                .or_else(|| stored.download_dir.clone())
                .unwrap_or_else(|| self.defaults.download_dir.clone()),
            close_to_tray: overrides
                .close_to_tray
                .or(stored.close_to_tray)
                .unwrap_or(true),
            plugins: self
                .sections
                .iter()
                .map(|section| (section.id.to_owned(), self.resolve(section)))
                .collect(),
        }
    }

    /// One plugin's section in effect, if it has one.
    pub(crate) fn section(&self, id: &str) -> Option<Value> {
        let section = self.sections.iter().find(|section| section.id == id)?;
        Some(self.resolve(section))
    }

    fn resolve(&self, section: &SettingsSection) -> Value {
        let empty = Map::new();
        let stored = self.stored.plugins.get(section.id).unwrap_or(&empty);
        section.resolve(stored).unwrap_or_else(|| {
            // A hand-edited file may hold what the plugin can't read; fall
            // back to the defaults until the user changes the section.
            tracing::warn!(section = section.id, "ignoring invalid stored settings");
            section.resolve(&empty).unwrap_or(Value::Null)
        })
    }

    /// Validate and apply `patch`, persisting the result before it takes
    /// effect. On any error nothing changes.
    pub(crate) fn update(&mut self, patch: SettingsPatch) -> Result<SettingsSnapshot, CoreError> {
        let mut stored = self.stored.clone();
        let mut overrides = self.overrides.clone();

        if let Some(value) = patch.device_name {
            let value = value.map(|name| name.trim().to_owned());
            if value
                .as_deref()
                .is_some_and(|name| !is_valid_device_name(name))
            {
                return Err(CoreError::InvalidDeviceName);
            }
            stored.device_name = value;
            overrides.device_name = None;
        }
        if let Some(value) = patch.download_dir {
            if let Some(directory) = &value {
                // Create it now, so an unusable directory is reported here
                // rather than as a failed transfer later.
                if !directory.is_absolute() || std::fs::create_dir_all(directory).is_err() {
                    return Err(CoreError::InvalidDownloadDir);
                }
            }
            stored.download_dir = value;
            overrides.download_dir = None;
        }
        if let Some(value) = patch.close_to_tray {
            stored.close_to_tray = value;
            overrides.close_to_tray = None;
        }
        for (id, change) in patch.plugins {
            let section = self
                .sections
                .iter()
                .find(|section| section.id == id)
                .ok_or(CoreError::InvalidSettings)?;
            let mut fields = stored.plugins.remove(&id).unwrap_or_default();
            match change {
                Value::Null => fields.clear(),
                Value::Object(changes) => {
                    for (field, value) in changes {
                        if value.is_null() {
                            fields.remove(&field);
                        } else {
                            fields.insert(field, value);
                        }
                    }
                }
                _ => return Err(CoreError::InvalidSettings),
            }
            if section.resolve(&fields).is_none() {
                return Err(CoreError::InvalidSettings);
            }
            if !fields.is_empty() {
                stored.plugins.insert(id, fields);
            }
        }

        if stored != self.stored
            && let Some(file) = &self.file
        {
            file.save(&stored).map_err(CoreError::Settings)?;
        }
        self.stored = stored;
        self.overrides = overrides;
        Ok(self.snapshot())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> SettingsDefaults {
        SettingsDefaults {
            device_name: "host".into(),
            download_dir: "/downloads".into(),
        }
    }

    fn patch(json: &str) -> SettingsPatch {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn start_options_override_stored_values_until_the_user_changes_them() {
        let directory = tempfile::tempdir().unwrap();
        let file = SettingsFile::new(directory.path());
        let stored = StoredSettings {
            device_name: Some("Stored".into()),
            ..Default::default()
        };
        let mut settings = Settings::new(defaults())
            .with_file(file.clone(), stored)
            .with_overrides(StoredSettings {
                device_name: Some("Flag".into()),
                ..Default::default()
            });

        let snapshot = settings.snapshot();
        assert_eq!(snapshot.device_name, "Flag");
        assert_eq!(snapshot.download_dir, PathBuf::from("/downloads"));
        assert!(snapshot.close_to_tray);

        // Changing another setting keeps the override and doesn't persist it.
        settings.update(patch(r#"{"closeToTray": false}"#)).unwrap();
        assert_eq!(settings.snapshot().device_name, "Flag");
        assert_eq!(file.load().unwrap().device_name.as_deref(), Some("Stored"));

        let snapshot = settings
            .update(patch(r#"{"deviceName": "  Renamed "}"#))
            .unwrap();
        assert_eq!(snapshot.device_name, "Renamed");
        let saved = file.load().unwrap();
        assert_eq!(saved.device_name.as_deref(), Some("Renamed"));
        assert_eq!(saved.close_to_tray, Some(false));
    }

    #[test]
    fn null_resets_a_setting_to_its_default() {
        let directory = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(defaults());
        let custom = directory.path().join("incoming");
        let body = serde_json::json!({ "downloadDir": custom, "deviceName": "Desk" });
        settings.update(patch(&body.to_string())).unwrap();
        assert!(custom.is_dir(), "the directory is created up front");
        assert_eq!(settings.snapshot().download_dir, custom);

        let snapshot = settings.update(patch(r#"{"downloadDir": null}"#)).unwrap();
        assert_eq!(snapshot.download_dir, PathBuf::from("/downloads"));
        assert_eq!(snapshot.device_name, "Desk");
    }

    #[test]
    fn invalid_values_change_nothing() {
        let mut settings = Settings::new(defaults());
        for (body, expected) in [
            (r#"{"deviceName": ""}"#, "InvalidDeviceName"),
            (r#"{"deviceName": "a.b"}"#, "InvalidDeviceName"),
            (
                r#"{"deviceName": "far too long for a device name, really"}"#,
                "InvalidDeviceName",
            ),
            (
                r#"{"closeToTray": false, "downloadDir": "relative/dir"}"#,
                "InvalidDownloadDir",
            ),
        ] {
            let error = settings.update(patch(body)).unwrap_err();
            assert_eq!(format!("{error:?}"), expected, "{body}");
        }
        assert_eq!(settings.snapshot(), Settings::new(defaults()).snapshot());
    }

    /// A plugin's settings, as a plugin would declare them.
    #[derive(Serialize, Deserialize)]
    #[serde(default, rename_all = "camelCase", deny_unknown_fields)]
    struct Waving {
        enabled: bool,
        hand: String,
    }

    impl Default for Waving {
        fn default() -> Self {
            Self {
                enabled: true,
                hand: "left".into(),
            }
        }
    }

    impl super::super::PluginSettings for Waving {
        const ID: &'static str = "wave";
    }

    #[test]
    fn plugin_sections_merge_changes_and_reset_to_their_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let file = SettingsFile::new(directory.path());
        let mut settings = Settings::new(defaults())
            .with_file(
                file.clone(),
                StoredSettings {
                    plugins: BTreeMap::from([(
                        "wave".into(),
                        Map::from_iter([("enabled".into(), false.into())]),
                    )]),
                    ..Default::default()
                },
            )
            .with_sections(vec![SettingsSection::of::<Waving>()]);

        let expected = serde_json::json!({"enabled": false, "hand": "left"});
        assert_eq!(settings.snapshot().plugins["wave"], expected);
        assert_eq!(settings.section("wave"), Some(expected));
        assert_eq!(settings.section("other"), None);

        let snapshot = settings
            .update(patch(r#"{"plugins": {"wave": {"hand": "right"}}}"#))
            .unwrap();
        assert_eq!(
            snapshot.plugins["wave"],
            serde_json::json!({"enabled": false, "hand": "right"})
        );
        // Only what the user set is saved.
        assert_eq!(
            serde_json::to_value(&file.load().unwrap().plugins).unwrap(),
            serde_json::json!({"wave": {"enabled": false, "hand": "right"}})
        );

        let snapshot = settings
            .update(patch(r#"{"plugins": {"wave": {"enabled": null}}}"#))
            .unwrap();
        assert_eq!(
            snapshot.plugins["wave"],
            serde_json::json!({"enabled": true, "hand": "right"})
        );

        let snapshot = settings
            .update(patch(r#"{"plugins": {"wave": null}}"#))
            .unwrap();
        assert_eq!(
            snapshot.plugins["wave"],
            serde_json::json!({"enabled": true, "hand": "left"})
        );
        assert!(file.load().unwrap().plugins.is_empty());
    }

    #[test]
    fn invalid_plugin_sections_change_nothing() {
        let mut settings =
            Settings::new(defaults()).with_sections(vec![SettingsSection::of::<Waving>()]);
        let before = settings.snapshot();
        for body in [
            r#"{"plugins": {"wave": {"hand": 1}}}"#,
            r#"{"plugins": {"wave": {"foot": "left"}}}"#,
            r#"{"plugins": {"wave": true}}"#,
            r#"{"plugins": {"unknown": {}}}"#,
            r#"{"closeToTray": false, "plugins": {"wave": {"enabled": "no"}}}"#,
        ] {
            let error = settings.update(patch(body)).unwrap_err();
            assert_eq!(format!("{error:?}"), "InvalidSettings", "{body}");
        }
        assert_eq!(settings.snapshot(), before);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(serde_json::from_str::<SettingsPatch>(r#"{"deviceNmae": "x"}"#).is_err());
        assert_eq!(patch("{}"), SettingsPatch::default());
    }
}
