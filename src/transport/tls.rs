//! Protocol-v8 TLS transport: mutually authenticated TLS 1.2/1.3 with
//! certificate pinning instead of certificate-authority validation.
//!
//! KDE Connect devices use long-lived self-signed certificates. There is no
//! certificate authority, so trust is established out of band by pairing and
//! then enforced by pinning the exact certificate bytes on every subsequent
//! connection. This module never skips handshake-signature verification: the
//! custom verifiers below always delegate to rustls's real signature checks
//! and only add device-id and pin comparisons on top of that.

use std::sync::Arc;

use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, DistinguishedName, Error as RustlsError,
    ServerConfig, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature},
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
    server::danger::{ClientCertVerified, ClientCertVerifier},
    version::{TLS12, TLS13},
};
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_rustls::{
    TlsAcceptor, TlsConnector, client::TlsStream as ClientTlsStream,
    server::TlsStream as ServerTlsStream,
};
use x509_parser::parse_x509_certificate;

/// A placeholder TLS server name. Hostname verification is meaningless for
/// KDE Connect's self-signed, CN-as-device-id certificates, so every
/// connection uses the same fixed name and identity is instead established
/// through [`DeviceIdentityVerifier`].
fn placeholder_server_name() -> ServerName<'static> {
    ServerName::try_from("myconnect.invalid").expect("static name is valid")
}

/// Local certificate and private key material in the shapes rustls expects.
pub struct TlsMaterial {
    certificate: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
}

impl TlsMaterial {
    pub fn new(certificate_der: &[u8], private_key_der: &[u8]) -> Self {
        Self {
            certificate: CertificateDer::from(certificate_der.to_vec()),
            key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(private_key_der.to_vec())),
        }
    }
}

/// Which certificate is acceptable from the peer during this handshake.
#[derive(Clone, Debug)]
pub enum PeerPin {
    /// No certificate has been pinned yet (peer is not a trusted device).
    /// Any certificate is accepted as long as its Common Name matches the
    /// device ID observed in the pre-TLS identity exchange.
    Unpinned,
    /// The peer must present exactly this certificate. Used for already
    /// trusted (paired) devices so a certificate change or downgrade fails
    /// closed instead of silently trusting a new key.
    Pinned(Vec<u8>),
}

/// Extract the DER-encoded SubjectPublicKeyInfo from a full certificate.
pub fn subject_public_key_info(certificate_der: &[u8]) -> Result<Vec<u8>, TlsError> {
    let (remaining, certificate) =
        parse_x509_certificate(certificate_der).map_err(|_| TlsError::InvalidCertificate)?;
    if !remaining.is_empty() {
        return Err(TlsError::InvalidCertificate);
    }
    Ok(certificate.public_key().raw.to_vec())
}

#[derive(Debug)]
struct DeviceIdentityVerifier {
    expected_device_id: String,
    pin: PeerPin,
    provider: Arc<CryptoProvider>,
}

impl DeviceIdentityVerifier {
    fn new(expected_device_id: String, pin: PeerPin, provider: Arc<CryptoProvider>) -> Self {
        Self {
            expected_device_id,
            pin,
            provider,
        }
    }

    fn check_certificate(&self, end_entity: &CertificateDer<'_>) -> Result<(), RustlsError> {
        if let PeerPin::Pinned(expected) = &self.pin
            && end_entity.as_ref() != expected.as_slice()
        {
            return Err(RustlsError::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ));
        }

        let (remaining, certificate) = parse_x509_certificate(end_entity.as_ref())
            .map_err(|_| RustlsError::InvalidCertificate(CertificateError::BadEncoding))?;
        if !remaining.is_empty() {
            return Err(RustlsError::InvalidCertificate(
                CertificateError::BadEncoding,
            ));
        }
        let mut common_names = certificate.subject().iter_common_name();
        let common_name = common_names.next().and_then(|value| value.as_str().ok());
        if common_name != Some(self.expected_device_id.as_str()) || common_names.next().is_some() {
            return Err(RustlsError::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ));
        }
        Ok(())
    }
}

impl ServerCertVerifier for DeviceIdentityVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        self.check_certificate(end_entity)?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

impl ClientCertVerifier for DeviceIdentityVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, RustlsError> {
        self.check_certificate(end_entity)?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn build_client_config(
    material: &TlsMaterial,
    expected_device_id: &str,
    pin: PeerPin,
) -> Result<ClientConfig, TlsError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(DeviceIdentityVerifier::new(
        expected_device_id.to_owned(),
        pin,
        provider.clone(),
    ));
    ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&TLS12, &TLS13])
        .map_err(TlsError::Config)?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(vec![material.certificate.clone()], material.key.clone_key())
        .map_err(TlsError::Config)
}

fn build_server_config(
    material: &TlsMaterial,
    expected_device_id: &str,
    pin: PeerPin,
) -> Result<ServerConfig, TlsError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(DeviceIdentityVerifier::new(
        expected_device_id.to_owned(),
        pin,
        provider.clone(),
    ));
    ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&TLS12, &TLS13])
        .map_err(TlsError::Config)?
        .with_client_cert_verifier(verifier)
        .with_single_cert(vec![material.certificate.clone()], material.key.clone_key())
        .map_err(TlsError::Config)
}

/// Upgrade an outgoing (client-role) TCP connection to mutually
/// authenticated TLS, verifying the peer's certificate against `pin`.
pub async fn connect(
    stream: TcpStream,
    material: &TlsMaterial,
    expected_device_id: &str,
    pin: PeerPin,
) -> Result<ClientTlsStream<TcpStream>, TlsError> {
    let config = build_client_config(material, expected_device_id, pin)?;
    let connector = TlsConnector::from(Arc::new(config));
    connector
        .connect(placeholder_server_name(), stream)
        .await
        .map_err(TlsError::Handshake)
}

/// Upgrade an incoming (server-role) TCP connection to mutually
/// authenticated TLS, verifying the peer's certificate against `pin`.
pub async fn accept(
    stream: TcpStream,
    material: &TlsMaterial,
    expected_device_id: &str,
    pin: PeerPin,
) -> Result<ServerTlsStream<TcpStream>, TlsError> {
    let config = build_server_config(material, expected_device_id, pin)?;
    let acceptor = TlsAcceptor::from(Arc::new(config));
    acceptor.accept(stream).await.map_err(TlsError::Handshake)
}

/// Extract the single peer certificate presented during the handshake.
pub fn client_peer_certificate(stream: &ClientTlsStream<TcpStream>) -> Result<Vec<u8>, TlsError> {
    let (_, session) = stream.get_ref();
    let certificates = session
        .peer_certificates()
        .ok_or(TlsError::MissingPeerCertificate)?;
    match certificates {
        [only] => Ok(only.as_ref().to_vec()),
        _ => Err(TlsError::MissingPeerCertificate),
    }
}

pub fn server_peer_certificate(stream: &ServerTlsStream<TcpStream>) -> Result<Vec<u8>, TlsError> {
    let (_, session) = stream.get_ref();
    let certificates = session
        .peer_certificates()
        .ok_or(TlsError::MissingPeerCertificate)?;
    match certificates {
        [only] => Ok(only.as_ref().to_vec()),
        _ => Err(TlsError::MissingPeerCertificate),
    }
}

#[derive(Debug, Error)]
pub enum TlsError {
    #[error("TLS configuration failed")]
    Config(#[source] RustlsError),
    #[error("TLS handshake failed")]
    Handshake(#[source] std::io::Error),
    #[error("peer certificate is invalid")]
    InvalidCertificate,
    #[error("peer did not present exactly one certificate")]
    MissingPeerCertificate,
}
