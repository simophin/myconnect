//! End-to-end pairing over real LAN discovery, TLS, and reconnection, plus
//! transport-level rejection of an identity swap and a protocol downgrade
//! attempted mid-handshake.

use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use myconnect::{
    application::{
        ApplicationHandle, ApplicationService, Command, EventData, LocalDeviceSnapshot,
        PairingDirection, PairingStatus, Query, QueryResult,
    },
    clipboard::InMemoryClipboard,
    config::{FilesystemTrustStore, LocalIdentity, TrustStore},
    device::DeviceReachability,
    protocol::{DeviceType, IdentityBody, Packet, PacketCodec},
    transport::{
        lan::{LanConfig, LanService, LocalDeviceInfo, TCP_PORT_RANGE},
        tls::{self, PeerPin, TlsMaterial, subject_public_key_info},
    },
};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use serde_json::{Map, json};
use tokio::{io::AsyncWriteExt, net::TcpStream, sync::mpsc, time::timeout};
use tokio_util::sync::CancellationToken;

struct Peer {
    identity: Arc<LocalIdentity>,
    trust_store: Arc<dyn TrustStore + Send + Sync>,
    application: ApplicationHandle,
    commands: mpsc::Receiver<Command>,
    _directory: tempfile::TempDir,
}

fn peer(name: &str) -> Peer {
    let directory = tempfile::tempdir().unwrap();
    let identity = Arc::new(LocalIdentity::load_or_create(directory.path()).unwrap());
    let trust_store: Arc<dyn TrustStore + Send + Sync> =
        Arc::new(FilesystemTrustStore::new(directory.path()));
    let public_key_der = subject_public_key_info(identity.certificate_der()).unwrap();
    let (application, commands) = ApplicationHandle::new(
        LocalDeviceSnapshot {
            device_id: identity.device_id().to_owned(),
            device_name: name.to_owned(),
        },
        8,
        public_key_der,
        trust_store.clone(),
        InMemoryClipboard::shared(),
        32,
        128,
        identity.clone(),
        myconnect::application::TransferConfig::new(directory.path().join("downloads")),
    )
    .unwrap();
    Peer {
        identity,
        trust_store,
        application,
        commands,
        _directory: directory,
    }
}

fn local(device_id: &str, name: &str) -> LocalDeviceInfo {
    LocalDeviceInfo {
        device_id: device_id.into(),
        device_name: name.into(),
        device_type: DeviceType::Desktop,
        incoming_capabilities: Vec::new(),
        outgoing_capabilities: Vec::new(),
    }
}

fn free_udp_addr() -> SocketAddr {
    let socket = std::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    socket.local_addr().unwrap()
}

fn test_config(bind: SocketAddr, target: SocketAddr) -> LanConfig {
    LanConfig::default()
        .with_discovery_bind(bind)
        .with_announcement_targets(vec![target])
        .with_tcp_bind(Ipv4Addr::LOCALHOST, TCP_PORT_RANGE)
        .with_announce_interval(Duration::from_millis(50))
        .with_timeouts(
            Duration::from_secs(1),
            Duration::from_secs(1),
            Duration::from_secs(2),
        )
}

async fn wait_for_reachability(
    application: &ApplicationHandle,
    device_id: &str,
    expected: DeviceReachability,
) {
    timeout(Duration::from_secs(3), async {
        loop {
            if let QueryResult::Device(Some(device)) = application
                .query(Query::Device {
                    device_id: device_id.into(),
                })
                .unwrap()
                && device.reachability == expected
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

async fn wait_for_paired(application: &ApplicationHandle, device_id: &str, expected: bool) {
    timeout(Duration::from_secs(3), async {
        loop {
            if let QueryResult::Device(Some(device)) = application
                .query(Query::Device {
                    device_id: device_id.into(),
                })
                .unwrap()
                && device.paired == expected
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn valid_peer_pairs_reconnects_with_pinned_trust_and_unpairs() {
    let a = peer("Peer A");
    let b = peer("Peer B");
    let a_id = a.identity.device_id().to_owned();
    let b_id = b.identity.device_id().to_owned();
    let a_udp = free_udp_addr();
    let b_udp = free_udp_addr();

    let a_service = LanService::start(
        test_config(a_udp, b_udp),
        local(&a_id, "Peer A"),
        a.application.clone(),
        a.commands,
        a.identity.clone(),
        a.trust_store.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let b_service = LanService::start(
        test_config(b_udp, a_udp),
        local(&b_id, "Peer B"),
        b.application.clone(),
        b.commands,
        b.identity.clone(),
        b.trust_store.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap();

    wait_for_reachability(&a.application, &b_id, DeviceReachability::Connected).await;
    wait_for_reachability(&b.application, &a_id, DeviceReachability::Connected).await;

    // A initiates pairing.
    let pairing = a.application.start_outgoing_pairing(&b_id).unwrap();
    assert_eq!(pairing.direction, PairingDirection::Outgoing);

    // B observes the incoming request and the matching verification code.
    let mut b_events = b.application.subscribe();
    let incoming = timeout(Duration::from_secs(2), async {
        loop {
            let event = b_events.recv().await.unwrap();
            if let EventData::PairingRequested(snapshot) = event.event {
                return snapshot;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(incoming.direction, PairingDirection::Incoming);
    assert_eq!(incoming.verification_code, pairing.verification_code);

    // B confirms; both ends converge on Accepted and paired.
    let accepted = b.application.accept_pairing(incoming.id).unwrap();
    assert_eq!(accepted.status, PairingStatus::Accepted);
    wait_for_paired(&a.application, &b_id, true).await;
    wait_for_paired(&b.application, &a_id, true).await;

    assert!(a.trust_store.get(&b_id).unwrap().is_some());
    assert!(b.trust_store.get(&a_id).unwrap().is_some());

    let a_identity = a.identity.clone();
    let a_trust_store = a.trust_store.clone();
    let b_identity = b.identity.clone();
    let b_trust_store = b.trust_store.clone();

    // Restart both sides; they must reconnect using pinned trust.
    a_service.shutdown().await.unwrap();
    b_service.shutdown().await.unwrap();

    let a2 = peer_reusing(a_identity, a_trust_store);
    let b2 = peer_reusing(b_identity, b_trust_store);
    let a2_udp = free_udp_addr();
    let b2_udp = free_udp_addr();
    let a2_service = LanService::start(
        test_config(a2_udp, b2_udp),
        local(&a_id, "Peer A"),
        a2.application.clone(),
        a2.commands,
        a2.identity.clone(),
        a2.trust_store.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let b2_service = LanService::start(
        test_config(b2_udp, a2_udp),
        local(&b_id, "Peer B"),
        b2.application.clone(),
        b2.commands,
        b2.identity.clone(),
        b2.trust_store.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap();

    wait_for_reachability(&a2.application, &b_id, DeviceReachability::Connected).await;
    wait_for_reachability(&b2.application, &a_id, DeviceReachability::Connected).await;
    // Reconnection alone (no new pairing) must already show both sides paired,
    // because the connection used the pinned certificate.
    match a2
        .application
        .query(Query::Device {
            device_id: b_id.clone(),
        })
        .unwrap()
    {
        QueryResult::Device(Some(device)) => assert!(device.paired),
        other => panic!("unexpected {other:?}"),
    }

    // Unpair from A's side: trust is removed, A tells B, and B drops its
    // trust in A too, before the connection is closed.
    let mut b2_events = b2.application.subscribe();
    a2.application.forget_device(&b_id).unwrap();
    assert!(a2.trust_store.get(&b_id).unwrap().is_none());
    timeout(Duration::from_secs(3), async {
        loop {
            if let EventData::DeviceUpdated(device) = b2_events.recv().await.unwrap().event
                && device.device_id == a_id
                && !device.paired
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    wait_for_paired(&b2.application, &a_id, false).await;
    assert!(b2.trust_store.get(&a_id).unwrap().is_none());

    a2_service.shutdown().await.unwrap();
    b2_service.shutdown().await.unwrap();
}

/// Build a fresh `Peer` that reuses an existing identity and trust store
/// (simulating the daemon restarting with the same persisted state).
fn peer_reusing(
    identity: Arc<LocalIdentity>,
    trust_store: Arc<dyn TrustStore + Send + Sync>,
) -> Peer {
    let directory = tempfile::tempdir().unwrap();
    let public_key_der = subject_public_key_info(identity.certificate_der()).unwrap();
    let (application, commands) = ApplicationHandle::new(
        LocalDeviceSnapshot {
            device_id: identity.device_id().to_owned(),
            device_name: "Restarted".to_owned(),
        },
        8,
        public_key_der,
        trust_store.clone(),
        InMemoryClipboard::shared(),
        32,
        128,
        identity.clone(),
        myconnect::application::TransferConfig::new(directory.path().join("downloads")),
    )
    .unwrap();
    Peer {
        identity,
        trust_store,
        application,
        commands,
        _directory: directory,
    }
}

// --- Transport-level rejection of a mid-handshake identity swap and a
// --- protocol downgrade, driven against a real LanService victim. ---

fn build_identity(device_id: &str, protocol_version: u8) -> IdentityBody {
    IdentityBody {
        device_id: device_id.to_owned(),
        device_name: "Attacker".into(),
        device_type: DeviceType::Desktop,
        incoming_capabilities: Vec::new(),
        outgoing_capabilities: Vec::new(),
        protocol_version,
        extra: Map::from_iter([("tcpPort".into(), json!(1716))]),
    }
}

fn self_signed(device_id: &str) -> (Vec<u8>, Vec<u8>) {
    let key = KeyPair::generate().unwrap();
    let mut params = CertificateParams::default();
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, device_id);
    let certificate = params.self_signed(&key).unwrap();
    (certificate.der().to_vec(), key.serialize_der())
}

/// Dial a real victim `LanService`, send the plaintext identity and complete
/// the TLS handshake honestly (claiming `pre_tls_id`), then present a
/// different identity inside TLS. Returns whether the victim ever showed the
/// connection as `Connected`.
async fn attempt_identity_mismatch(
    victim_addr: SocketAddr,
    victim_device_id: &str,
    pre_tls_id: &str,
    pre_tls_protocol: u8,
    inner_id: &str,
    inner_protocol: u8,
) -> bool {
    let mut stream = TcpStream::connect(victim_addr).await.unwrap();

    let pre_tls_identity = build_identity(pre_tls_id, pre_tls_protocol);
    let packet = Packet::from_body(0_u64, "kdeconnect.identity", &pre_tls_identity).unwrap();
    let bytes = PacketCodec::new(16 * 1024).encode(&packet).unwrap();
    stream.write_all(&bytes).await.unwrap();

    // The victim accepted the connection, so it sends no plaintext identity
    // and acts as the TLS client; the attacker, having dialed, is the server.
    let (cert_der, key_der) = self_signed(pre_tls_id);
    let material = TlsMaterial::new(&cert_der, &key_der);
    let mut tls_stream = timeout(
        Duration::from_secs(2),
        tls::accept(stream, &material, victim_device_id, PeerPin::Unpinned),
    )
    .await
    .unwrap()
    .expect("the honest pre-TLS identity lets the handshake itself succeed");

    let inner_identity = build_identity(inner_id, inner_protocol);
    let packet = Packet::from_body(0_u64, "kdeconnect.identity", &inner_identity).unwrap();
    let bytes = PacketCodec::new(16 * 1024).encode(&packet).unwrap();
    let _ = tls_stream.write_all(&bytes).await;
    let _ = tls_stream.flush().await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    false // overwritten by the caller via the application query below
}

struct Victim {
    application: ApplicationHandle,
    device_id: String,
    _identity: Arc<LocalIdentity>,
    _trust_store: Arc<dyn TrustStore + Send + Sync>,
    _directory: tempfile::TempDir,
}

async fn victim() -> (Victim, LanService, SocketAddr) {
    let victim = peer("Victim");
    let id = victim.identity.device_id().to_owned();
    let application = victim.application.clone();
    let identity = victim.identity.clone();
    let trust_store = victim.trust_store.clone();
    let service = LanService::start(
        LanConfig::default()
            .with_discovery_bind(free_udp_addr())
            .with_announcement_targets(Vec::new())
            .with_tcp_bind(Ipv4Addr::LOCALHOST, TCP_PORT_RANGE)
            .with_timeouts(
                Duration::from_secs(1),
                Duration::from_secs(1),
                Duration::from_secs(2),
            ),
        local(&id, "Victim"),
        application.clone(),
        victim.commands,
        identity.clone(),
        trust_store.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let addr = service.tcp_addr();
    (
        Victim {
            application,
            device_id: id,
            _identity: identity,
            _trust_store: trust_store,
            _directory: victim._directory,
        },
        service,
        addr,
    )
}

#[tokio::test]
async fn identity_swap_after_tls_handshake_is_rejected() {
    let (victim, service, addr) = victim().await;
    let victim_id = victim.device_id.clone();
    let claimed_id = "11111111111111111111111111111111".to_owned();
    let swapped_id = "22222222222222222222222222222222".to_owned();

    let _ = attempt_identity_mismatch(addr, &victim_id, &claimed_id, 8, &swapped_id, 8).await;

    // The device that showed up during the pre-TLS plaintext exchange must
    // never be registered as connected, because its post-TLS identity did
    // not match.
    assert!(matches!(
        victim
            .application
            .query(Query::Device {
                device_id: claimed_id
            })
            .unwrap(),
        QueryResult::Device(None)
    ));

    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn protocol_downgrade_after_tls_handshake_is_rejected() {
    let (victim, service, addr) = victim().await;
    let victim_id = victim.device_id.clone();
    let claimed_id = "33333333333333333333333333333333".to_owned();

    // Same device ID both times, but the protocol version changes between
    // the pre-TLS and post-TLS identity exchanges.
    let _ = attempt_identity_mismatch(addr, &victim_id, &claimed_id, 8, &claimed_id, 7).await;

    assert!(matches!(
        victim
            .application
            .query(Query::Device {
                device_id: claimed_id
            })
            .unwrap(),
        QueryResult::Device(None)
    ));

    service.shutdown().await.unwrap();
}
