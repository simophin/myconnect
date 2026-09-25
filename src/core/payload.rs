//! Payload connections for plugins: the auxiliary connections a file's
//! bytes travel over, beside a device's control connection.
//!
//! A plugin gets a [`PayloadPeer`] from [`super::PluginContext::payload_peer`]
//! and either listens for the device to dial it ([`PayloadPeer::listen`]) or
//! dials a port the device advertised ([`PayloadPeer::connect`]). Both sides
//! authenticate with this device's certificate and pin the peer's, as the
//! control connection does. A plugin that speaks SSH to a server on the
//! device (browsing its files) signs in with [`PayloadPeer::authenticate_ssh`]
//! and checks the server's host key against
//! [`PayloadPeer::certificate_der`]. The private key stays inside this
//! module: a plugin gets connected streams and signed-in sessions, never the
//! key material.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use russh::{
    client::{AuthResult, Handle, Handler},
    keys::{PrivateKeyWithHashAlg, pkcs8::decode_pkcs8},
};
use thiserror::Error;
use tokio::net::{TcpListener, TcpStream};

use crate::{
    config::LocalIdentity,
    transport::{
        lan::TCP_PORT_RANGE,
        payload::{self, PayloadError},
        tls::{PeerPin, TlsMaterial},
    },
};

/// The TLS stream of a payload connection this device accepted.
pub type AcceptedPayload = tokio_rustls::server::TlsStream<TcpStream>;
/// The TLS stream of a payload connection this device dialed.
pub type DialedPayload = tokio_rustls::client::TlsStream<TcpStream>;

/// What it takes to open authenticated connections with one paired,
/// connected device, captured when it was asked for.
#[derive(Clone)]
pub struct PayloadPeer {
    device_id: String,
    certificate_der: Vec<u8>,
    /// Where the device's control connection comes from; payload ports it
    /// advertises are on the same host. `None` for a connection registered
    /// without a real transport (some tests).
    ip: Option<IpAddr>,
    identity: Arc<LocalIdentity>,
    bind_ip: Ipv4Addr,
    timeout: Duration,
}

impl PayloadPeer {
    pub(super) fn new(
        device_id: String,
        certificate_der: Vec<u8>,
        ip: Option<IpAddr>,
        identity: Arc<LocalIdentity>,
        bind_ip: Ipv4Addr,
        timeout: Duration,
    ) -> Self {
        Self {
            device_id,
            certificate_der,
            ip,
            identity,
            bind_ip,
            timeout,
        }
    }

    /// The address of the device's host, where its payload and file-server
    /// ports are; `None` if the core doesn't know it (a connection
    /// registered without a real transport, in tests).
    pub fn ip(&self) -> Option<IpAddr> {
        self.ip
    }

    /// The certificate pinned for the device at pairing. Its public key is
    /// also the device's SSH host key.
    pub fn certificate_der(&self) -> &[u8] {
        &self.certificate_der
    }

    /// Sign in to an SSH server on the device as `user`, with this device's
    /// key, which KDE Connect accepts from a paired device. `Ok(false)` if
    /// the server refused the key. The caller has connected `ssh` and
    /// checked its host key.
    pub async fn authenticate_ssh<H: Handler>(
        &self,
        ssh: &mut Handle<H>,
        user: &str,
    ) -> Result<bool, SshAuthError> {
        let key = decode_pkcs8(self.identity.private_key_der(), None)
            .map_err(|_| SshAuthError::InvalidLocalKey)?;
        let hash_alg = if key.algorithm().is_rsa() {
            ssh.best_supported_rsa_hash().await?.flatten()
        } else {
            None
        };
        let result = ssh
            .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg))
            .await?;
        Ok(matches!(result, AuthResult::Success))
    }

    /// Bind a port in KDE Connect's range for the device to dial; advertise
    /// [`PayloadListener::port`] to it, then [`PayloadListener::accept`].
    pub async fn listen(&self) -> Result<PayloadListener, PayloadError> {
        let (listener, port) = payload::bind_payload_listener(self.bind_ip, TCP_PORT_RANGE).await?;
        Ok(PayloadListener {
            listener,
            port,
            peer: self.clone(),
        })
    }

    /// Dial `port` on the device's host and authenticate it as the device.
    pub async fn connect(&self, port: u16) -> Result<DialedPayload, PayloadError> {
        let ip = self.ip.ok_or_else(|| {
            PayloadError::Socket(std::io::Error::new(
                std::io::ErrorKind::AddrNotAvailable,
                "the device's address is unknown",
            ))
        })?;
        payload::connect_payload(
            SocketAddr::new(ip, port),
            self.timeout,
            &self.material(),
            &self.device_id,
            PeerPin::Pinned(self.certificate_der.clone()),
        )
        .await
    }

    fn material(&self) -> TlsMaterial {
        TlsMaterial::new(
            self.identity.certificate_der(),
            self.identity.private_key_der(),
        )
    }
}

/// A port bound for one payload connection from a device.
pub struct PayloadListener {
    listener: TcpListener,
    port: u16,
    peer: PayloadPeer,
}

impl PayloadListener {
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Wait, up to the payload connect timeout, for the device to dial in,
    /// and authenticate it.
    pub async fn accept(self) -> Result<AcceptedPayload, PayloadError> {
        let material = self.peer.material();
        payload::accept_payload_connection(
            self.listener,
            self.peer.timeout,
            &material,
            &self.peer.device_id,
            PeerPin::Pinned(self.peer.certificate_der),
        )
        .await
    }
}

/// Why [`PayloadPeer::authenticate_ssh`] couldn't try the key.
#[derive(Debug, Error)]
pub enum SshAuthError {
    #[error("this device's private key can't be used for SSH")]
    InvalidLocalKey,
    #[error("SSH connection failed")]
    Ssh(#[from] russh::Error),
}
