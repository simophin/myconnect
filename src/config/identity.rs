use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, PublicKeyData};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use x509_parser::parse_x509_certificate;

use super::{create_private_dir, private_file_options};

const IDENTITY_FILE: &str = "identity.json";

/// Persistent TLS identity used by the local MyConnect device.
///
/// This deliberately does not implement `Debug`, preventing accidental key
/// disclosure through otherwise harmless diagnostic formatting.
#[derive(Clone, PartialEq, Eq)]
pub struct LocalIdentity {
    device_id: String,
    certificate_der: Vec<u8>,
    private_key_der: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredIdentity {
    device_id: String,
    certificate_der: Vec<u8>,
    private_key_der: Vec<u8>,
}

impl LocalIdentity {
    /// Load identity material from `config_dir`, creating it atomically when it
    /// does not exist.
    pub fn load_or_create(config_dir: impl AsRef<Path>) -> Result<Self, IdentityError> {
        let config_dir = config_dir.as_ref();
        let path = config_dir.join(IDENTITY_FILE);
        match fs::read(&path) {
            Ok(bytes) => Self::from_stored(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                create_private_dir(config_dir).map_err(IdentityError::Io)?;
                let identity = Self::generate()?;
                identity.persist_atomically(&path)?;
                Ok(identity)
            }
            Err(error) => Err(IdentityError::Io(error)),
        }
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn certificate_der(&self) -> &[u8] {
        &self.certificate_der
    }

    pub fn private_key_der(&self) -> &[u8] {
        &self.private_key_der
    }

    fn generate() -> Result<Self, IdentityError> {
        let device_id = Uuid::new_v4().simple().to_string();
        if !is_local_device_id(&device_id) {
            return Err(IdentityError::InvalidDeviceId);
        }

        let signing_key = KeyPair::generate().map_err(|_| IdentityError::Generation)?;
        let mut params = CertificateParams::default();
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, device_id.as_str());
        let certificate = params
            .self_signed(&signing_key)
            .map_err(|_| IdentityError::Generation)?;

        let identity = Self {
            device_id,
            certificate_der: certificate.der().to_vec(),
            private_key_der: signing_key.serialize_der(),
        };
        identity.validate()?;
        Ok(identity)
    }

    fn from_stored(bytes: &[u8]) -> Result<Self, IdentityError> {
        let stored: StoredIdentity =
            serde_json::from_slice(bytes).map_err(|_| IdentityError::Corrupt)?;
        let identity = Self {
            device_id: stored.device_id,
            certificate_der: stored.certificate_der,
            private_key_der: stored.private_key_der,
        };
        identity.validate()?;
        Ok(identity)
    }

    fn validate(&self) -> Result<(), IdentityError> {
        if !is_local_device_id(&self.device_id) {
            return Err(IdentityError::InvalidDeviceId);
        }

        let (remaining, certificate) = parse_x509_certificate(&self.certificate_der)
            .map_err(|_| IdentityError::InvalidCertificate)?;
        if !remaining.is_empty() {
            return Err(IdentityError::InvalidCertificate);
        }

        let mut common_names = certificate.subject().iter_common_name();
        let common_name = common_names
            .next()
            .and_then(|value| value.as_str().ok())
            .ok_or(IdentityError::CertificateIdentityMismatch)?;
        if common_name != self.device_id || common_names.next().is_some() {
            return Err(IdentityError::CertificateIdentityMismatch);
        }
        if certificate.subject() != certificate.issuer() {
            return Err(IdentityError::InvalidCertificate);
        }

        let signing_key = KeyPair::try_from(self.private_key_der.as_slice())
            .map_err(|_| IdentityError::InvalidPrivateKey)?;
        if certificate.public_key().raw != signing_key.subject_public_key_info() {
            return Err(IdentityError::KeyCertificateMismatch);
        }

        Ok(())
    }

    fn persist_atomically(&self, destination: &Path) -> Result<(), IdentityError> {
        let stored = StoredIdentity {
            device_id: self.device_id.clone(),
            certificate_der: self.certificate_der.clone(),
            private_key_der: self.private_key_der.clone(),
        };
        let bytes = serde_json::to_vec(&stored).map_err(|_| IdentityError::Encoding)?;
        let temporary = temporary_path(destination);

        let result = (|| {
            let mut file = private_file_options()
                .open(&temporary)
                .map_err(IdentityError::Io)?;
            file.write_all(&bytes).map_err(IdentityError::Io)?;
            file.sync_all().map_err(IdentityError::Io)?;
            fs::rename(&temporary, destination).map_err(IdentityError::Io)?;
            sync_parent(destination).map_err(IdentityError::Io)
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn is_local_device_id(device_id: &str) -> bool {
    device_id.len() == 32 && device_id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn temporary_path(destination: &Path) -> PathBuf {
    destination.with_file_name(format!(".identity-{}.tmp", Uuid::new_v4().simple()))
}

#[cfg(unix)]
fn sync_parent(destination: &Path) -> std::io::Result<()> {
    fs::File::open(destination.parent().expect("identity has a parent"))?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent(_destination: &Path) -> std::io::Result<()> {
    Ok(())
}

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("identity storage operation failed")]
    Io(#[source] std::io::Error),
    #[error("identity file is corrupt")]
    Corrupt,
    #[error("identity could not be encoded")]
    Encoding,
    #[error("identity generation failed")]
    Generation,
    #[error("device ID must be exactly 32 hexadecimal characters")]
    InvalidDeviceId,
    #[error("identity certificate is invalid")]
    InvalidCertificate,
    #[error("certificate Common Name does not match the device ID")]
    CertificateIdentityMismatch,
    #[error("identity private key is invalid")]
    InvalidPrivateKey,
    #[error("identity private key does not match its certificate")]
    KeyCertificateMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_persistent_and_well_formed() {
        let directory = tempfile::tempdir().unwrap();
        let first = LocalIdentity::load_or_create(directory.path()).unwrap();
        let second = LocalIdentity::load_or_create(directory.path()).unwrap();

        assert_eq!(first.device_id(), second.device_id());
        assert_eq!(first.certificate_der(), second.certificate_der());
        assert_eq!(first.private_key_der(), second.private_key_der());
        assert_eq!(first.device_id().len(), 32);
        assert!(
            first
                .device_id()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
    }

    #[test]
    fn partial_identity_is_rejected_without_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(IDENTITY_FILE);
        fs::write(&path, br#"{"deviceId":"unfinished"}"#).unwrap();

        assert!(matches!(
            LocalIdentity::load_or_create(directory.path()),
            Err(IdentityError::Corrupt)
        ));
        assert_eq!(fs::read(path).unwrap(), br#"{"deviceId":"unfinished"}"#);
    }

    #[test]
    fn certificate_common_name_must_match_device_id() {
        let directory = tempfile::tempdir().unwrap();
        LocalIdentity::load_or_create(directory.path()).unwrap();
        let path = directory.path().join(IDENTITY_FILE);
        let mut stored: StoredIdentity = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        stored.device_id = "11111111111111111111111111111111".into();
        fs::write(path, serde_json::to_vec(&stored).unwrap()).unwrap();

        assert!(matches!(
            LocalIdentity::load_or_create(directory.path()),
            Err(IdentityError::CertificateIdentityMismatch)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn identity_file_has_private_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        LocalIdentity::load_or_create(directory.path()).unwrap();

        let mode = fs::metadata(directory.path().join(IDENTITY_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
    }
}
