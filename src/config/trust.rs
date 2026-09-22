use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use x509_parser::parse_x509_certificate;

use super::{create_private_dir, is_valid_device_id, private_file_options};

/// Persisted certificate pin for a paired peer.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedDevice {
    pub device_id: String,
    pub certificate_der: Vec<u8>,
    pub last_trusted_protocol_version: u8,
}

/// Storage abstraction for paired-device certificate pins.
pub trait TrustStore {
    fn get(&self, device_id: &str) -> Result<Option<TrustedDevice>, TrustError>;
    fn list(&self) -> Result<Vec<TrustedDevice>, TrustError>;
    fn put(&self, device: &TrustedDevice) -> Result<(), TrustError>;
    fn remove(&self, device_id: &str) -> Result<bool, TrustError>;
}

/// One-JSON-file-per-device trust storage under the configuration directory.
pub struct FilesystemTrustStore {
    directory: PathBuf,
}

impl FilesystemTrustStore {
    pub fn new(config_dir: impl AsRef<Path>) -> Self {
        Self {
            directory: config_dir.as_ref().join("trusted-devices"),
        }
    }

    fn path_for(&self, device_id: &str) -> Result<PathBuf, TrustError> {
        validate_device_id(device_id)?;
        Ok(self.directory.join(format!("{device_id}.json")))
    }
}

impl TrustStore for FilesystemTrustStore {
    fn get(&self, device_id: &str) -> Result<Option<TrustedDevice>, TrustError> {
        let path = self.path_for(device_id)?;
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(TrustError::Io(error)),
        };
        let device = decode_record(&bytes)?;
        if device.device_id != device_id {
            return Err(TrustError::Corrupt);
        }
        Ok(Some(device))
    }

    fn list(&self) -> Result<Vec<TrustedDevice>, TrustError> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(TrustError::Io(error)),
        };
        let mut devices = Vec::new();
        for entry in entries {
            let entry = entry.map_err(TrustError::Io)?;
            if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            devices.push(decode_record(
                &fs::read(entry.path()).map_err(TrustError::Io)?,
            )?);
        }
        devices.sort_by(|left, right| left.device_id.cmp(&right.device_id));
        Ok(devices)
    }

    fn put(&self, device: &TrustedDevice) -> Result<(), TrustError> {
        validate_record(device)?;
        create_private_dir(&self.directory).map_err(TrustError::Io)?;
        let destination = self.path_for(&device.device_id)?;
        let temporary = self
            .directory
            .join(format!(".trust-{}.tmp", Uuid::new_v4().simple()));
        let bytes = serde_json::to_vec(device).map_err(|_| TrustError::Encoding)?;

        let result = (|| {
            let mut file = private_file_options()
                .open(&temporary)
                .map_err(TrustError::Io)?;
            file.write_all(&bytes).map_err(TrustError::Io)?;
            file.sync_all().map_err(TrustError::Io)?;
            fs::rename(&temporary, destination).map_err(TrustError::Io)?;
            sync_directory(&self.directory).map_err(TrustError::Io)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn remove(&self, device_id: &str) -> Result<bool, TrustError> {
        let path = self.path_for(device_id)?;
        match fs::remove_file(path) {
            Ok(()) => {
                sync_directory(&self.directory).map_err(TrustError::Io)?;
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(TrustError::Io(error)),
        }
    }
}

fn decode_record(bytes: &[u8]) -> Result<TrustedDevice, TrustError> {
    let device = serde_json::from_slice(bytes).map_err(|_| TrustError::Corrupt)?;
    validate_record(&device)?;
    Ok(device)
}

fn validate_record(device: &TrustedDevice) -> Result<(), TrustError> {
    validate_device_id(&device.device_id)?;
    if !matches!(device.last_trusted_protocol_version, 7 | 8) {
        return Err(TrustError::UnsupportedProtocolVersion);
    }
    let (remaining, _) = parse_x509_certificate(&device.certificate_der)
        .map_err(|_| TrustError::InvalidCertificate)?;
    if !remaining.is_empty() {
        return Err(TrustError::InvalidCertificate);
    }
    Ok(())
}

fn validate_device_id(device_id: &str) -> Result<(), TrustError> {
    is_valid_device_id(device_id)
        .then_some(())
        .ok_or(TrustError::InvalidDeviceId)
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
pub enum TrustError {
    #[error("trust storage operation failed")]
    Io(#[source] std::io::Error),
    #[error("trust record is corrupt")]
    Corrupt,
    #[error("trust record could not be encoded")]
    Encoding,
    #[error("peer device ID is invalid")]
    InvalidDeviceId,
    #[error("peer certificate is invalid")]
    InvalidCertificate,
    #[error("peer protocol version is unsupported")]
    UnsupportedProtocolVersion,
}

#[cfg(test)]
mod tests {
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};

    use super::*;

    fn trusted_device(device_id: &str) -> TrustedDevice {
        let key = KeyPair::generate().unwrap();
        let mut params = CertificateParams::default();
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, device_id);
        let certificate = params.self_signed(&key).unwrap();
        TrustedDevice {
            device_id: device_id.into(),
            certificate_der: certificate.der().to_vec(),
            last_trusted_protocol_version: 8,
        }
    }

    #[test]
    fn trust_records_round_trip_and_can_be_removed() {
        let directory = tempfile::tempdir().unwrap();
        let store = FilesystemTrustStore::new(directory.path());
        let device = trusted_device("740bd4b9b4184ee497d6caf1da8151be");

        store.put(&device).unwrap();
        assert!(store.get(&device.device_id).unwrap().is_some());
        let listed = store.list().unwrap();
        assert!(listed.len() == 1 && listed[0] == device);
        assert!(store.remove(&device.device_id).unwrap());
        assert!(store.get(&device.device_id).unwrap().is_none());
        assert!(!store.remove(&device.device_id).unwrap());
    }

    #[test]
    fn unsafe_device_ids_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let store = FilesystemTrustStore::new(directory.path());
        assert!(matches!(
            store.get("../../not-a-device"),
            Err(TrustError::InvalidDeviceId)
        ));
    }
}
