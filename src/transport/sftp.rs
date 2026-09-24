//! SFTP client connection to a peer's file server (KDE Connect's `sftp`
//! plugin).
//!
//! The peer's SSH host key is its KDE Connect key pair: the key in the TLS
//! certificate pinned at pairing. The server is therefore authenticated
//! against that certificate. KDE Connect's own clients skip this check;
//! MyConnect doesn't. This device authenticates with its own TLS key, which
//! KDE Connect for Android accepts from the paired device. The one-off
//! password from the offer is only a fallback.

use std::{sync::Arc, time::Duration};

use russh::{
    client::{self, AuthResult, Handle},
    keys::{
        PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate, pkcs8::decode_pkcs8,
        ssh_key::public::KeyData,
    },
};
use russh_sftp::client::SftpSession;
use thiserror::Error;
use tokio::net::TcpStream;
use x509_parser::{parse_x509_certificate, public_key::PublicKey};

/// Where and as whom to connect, from a peer's `kdeconnect.sftp` offer.
pub struct SftpEndpoint {
    pub addr: std::net::SocketAddr,
    pub user: String,
    pub password: String,
}

/// An open SFTP session and the SSH connection carrying it. Dropping it
/// closes the connection.
pub struct SftpConnection {
    sftp: SftpSession,
    ssh: Handle<PinnedHostKey>,
}

impl SftpConnection {
    pub fn session(&self) -> &SftpSession {
        &self.sftp
    }

    /// Whether the SSH connection has ended (the peer closed it, or it
    /// failed). A closed connection can't be used again.
    pub fn is_closed(&self) -> bool {
        self.ssh.is_closed()
    }

    /// Close the session and the connection, telling the peer.
    pub async fn close(self) {
        let _ = self.sftp.close().await;
        let _ = self
            .ssh
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await;
    }
}

/// Connect to a peer's SFTP server and open an SFTP session.
///
/// `local_key_der` is this device's PKCS#8 private key (its TLS key), and
/// `peer_certificate_der` the certificate pinned for the peer. The whole
/// exchange, from TCP connect to the SFTP handshake, must finish within
/// `deadline`.
pub async fn connect(
    endpoint: &SftpEndpoint,
    local_key_der: &[u8],
    peer_certificate_der: &[u8],
    deadline: Duration,
) -> Result<SftpConnection, SftpError> {
    let key = Arc::new(decode_pkcs8(local_key_der, None).map_err(|_| SftpError::InvalidLocalKey)?);
    let expected = certificate_key_data(peer_certificate_der)?;
    tokio::time::timeout(deadline, open(endpoint, key, expected))
        .await
        .map_err(|_| SftpError::TimedOut)?
}

async fn open(
    endpoint: &SftpEndpoint,
    key: Arc<PrivateKey>,
    expected: KeyData,
) -> Result<SftpConnection, SftpError> {
    let config = Arc::new(client::Config {
        // Nothing else keeps an idle session alive, and the peer drops
        // silently when it leaves the network.
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 2,
        nodelay: true,
        ..Default::default()
    });
    let stream = TcpStream::connect(endpoint.addr)
        .await
        .map_err(SftpError::Connect)?;
    let mut ssh = client::connect_stream(config, stream, PinnedHostKey { expected }).await?;

    let hash_alg = if key.algorithm().is_rsa() {
        ssh.best_supported_rsa_hash().await?.flatten()
    } else {
        None
    };
    let by_key = ssh
        .authenticate_publickey(
            endpoint.user.clone(),
            PrivateKeyWithHashAlg::new(key, hash_alg),
        )
        .await?;
    if matches!(by_key, AuthResult::Success) {
        tracing::debug!("signed in to the file server with our key");
    } else {
        tracing::debug!("file server refused our key; trying the password");
        let by_password = ssh
            .authenticate_password(endpoint.user.clone(), endpoint.password.clone())
            .await?;
        if !matches!(by_password, AuthResult::Success) {
            return Err(SftpError::AuthenticationRejected);
        }
    }

    let channel = ssh.channel_open_session().await?;
    channel.request_subsystem(true, "sftp").await?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(SftpError::Sftp)?;
    Ok(SftpConnection { sftp, ssh })
}

/// The public key in `certificate_der`, in SSH form, for comparison with
/// the host key the server presents.
fn certificate_key_data(certificate_der: &[u8]) -> Result<KeyData, SftpError> {
    let (_, certificate) =
        parse_x509_certificate(certificate_der).map_err(|_| SftpError::UnsupportedPeerKey)?;
    match certificate
        .public_key()
        .parsed()
        .map_err(|_| SftpError::UnsupportedPeerKey)?
    {
        PublicKey::EC(point) => {
            russh::keys::ssh_key::public::EcdsaPublicKey::from_sec1_bytes(point.data())
                .map(KeyData::Ecdsa)
                .map_err(|_| SftpError::UnsupportedPeerKey)
        }
        PublicKey::RSA(rsa) => {
            let exponent = russh::keys::ssh_encoding::Mpint::from_positive_bytes(rsa.exponent);
            let modulus = russh::keys::ssh_encoding::Mpint::from_positive_bytes(rsa.modulus);
            russh::keys::ssh_key::public::RsaPublicKey::new(exponent, modulus)
                .map(KeyData::Rsa)
                .map_err(|_| SftpError::UnsupportedPeerKey)
        }
        _ => Err(SftpError::UnsupportedPeerKey),
    }
}

/// Accepts only the host key equal to the peer's pinned certificate key.
struct PinnedHostKey {
    expected: KeyData,
}

impl client::Handler for PinnedHostKey {
    type Error = SftpError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        match server_public_key {
            PublicKeyOrCertificate::PublicKey { key, .. } if *key.key_data() == self.expected => {
                Ok(true)
            }
            _ => Err(SftpError::HostKeyMismatch),
        }
    }
}

#[derive(Debug, Error)]
pub enum SftpError {
    #[error("this device's private key can't be used for SSH")]
    InvalidLocalKey,
    #[error("the peer's certificate key can't be used as an SSH host key")]
    UnsupportedPeerKey,
    #[error("SFTP server could not be reached")]
    Connect(#[source] std::io::Error),
    #[error("SFTP server's host key doesn't match the peer's certificate")]
    HostKeyMismatch,
    #[error("SFTP server rejected this device's credentials")]
    AuthenticationRejected,
    #[error("SFTP connection was not established before the deadline")]
    TimedOut,
    #[error("SSH connection failed")]
    Ssh(#[from] russh::Error),
    #[error("SFTP session failed")]
    Sftp(#[source] russh_sftp::client::error::Error),
}

#[cfg(test)]
mod tests {
    use rcgen::{CertificateParams, KeyPair};

    use super::*;

    #[test]
    fn a_certificate_key_matches_the_ssh_form_of_the_same_key() {
        let key_pair = KeyPair::generate().unwrap();
        let certificate = CertificateParams::default().self_signed(&key_pair).unwrap();
        let private = decode_pkcs8(&key_pair.serialize_der(), None).unwrap();

        let from_certificate = certificate_key_data(certificate.der()).unwrap();

        assert_eq!(&from_certificate, private.public_key().key_data());
        let other = KeyPair::generate().unwrap();
        assert_ne!(
            &from_certificate,
            decode_pkcs8(&other.serialize_der(), None)
                .unwrap()
                .public_key()
                .key_data()
        );
    }

    /// Older KDE Connect for Android installs have RSA keys. ring can't
    /// generate RSA keys, so this uses a fixture made with
    /// `openssl req -x509 -newkey rsa:2048`.
    #[test]
    fn an_rsa_certificate_key_matches_its_ssh_form() {
        let certificate = include_bytes!("testdata/rsa_certificate.der");
        let private = decode_pkcs8(include_bytes!("testdata/rsa_private_key.der"), None).unwrap();

        assert_eq!(
            &certificate_key_data(certificate).unwrap(),
            private.public_key().key_data()
        );
    }
}
