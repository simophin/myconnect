//! Payload connections for plugins: the auxiliary TLS connections a file's
//! bytes travel over, beside a device's control connection.
//!
//! A plugin gets a [`PayloadPeer`] from [`super::PluginContext::payload_peer`]
//! and either listens for the device to dial it ([`PayloadPeer::listen`]) or
//! dials a port the device advertised ([`PayloadPeer::connect`]). Both sides
//! authenticate with this device's certificate and pin the peer's, as the
//! control connection does. The private key stays inside this module: a
//! plugin gets connected streams, never the key material.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

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

/// What it takes to open payload connections with one paired, connected
/// device, captured when it was asked for.
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
