//! End-to-end ping vertical slice: LAN discovery, protocol-v8 TLS pairing,
//! and encrypted plugin dispatch of `kdeconnect.ping`, plus capability
//! filtering that rejects a ping to a peer that never advertised support.

use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use myconnect::{
    application::{
        ApplicationError, ApplicationHandle, ApplicationService, Command, LocalDeviceSnapshot,
        Query, QueryResult,
    },
    clipboard::InMemoryClipboard,
    config::{FilesystemTrustStore, LocalIdentity, TrustStore},
    device::DeviceReachability,
    plugins,
    protocol::DeviceType,
    transport::{
        lan::{LanConfig, LanService, LocalDeviceInfo, TCP_PORT_RANGE},
        tls::subject_public_key_info,
    },
};
use tokio::{sync::mpsc, time::timeout};
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

/// A `LocalDeviceInfo` that advertises the ping capability in both
/// directions, as production code does via `plugins::capabilities()`.
fn local_with_ping(device_id: &str, name: &str) -> LocalDeviceInfo {
    let capabilities = plugins::capabilities();
    LocalDeviceInfo {
        device_id: device_id.into(),
        device_name: name.into(),
        device_type: DeviceType::Desktop,
        incoming_capabilities: capabilities.incoming,
        outgoing_capabilities: capabilities.outgoing,
    }
}

/// A `LocalDeviceInfo` that advertises no plugin capabilities at all, used
/// to prove capability filtering rejects an outgoing ping.
fn local_without_capabilities(device_id: &str, name: &str) -> LocalDeviceInfo {
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

async fn wait_for_ping(application: &ApplicationHandle, device_id: &str, expected_message: &str) {
    timeout(Duration::from_secs(3), async {
        loop {
            if let Some(body) = application.last_ping_received(device_id)
                && body.message.as_deref() == Some(expected_message)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

async fn pair(a: &ApplicationHandle, b: &ApplicationHandle, a_id: &str, b_id: &str) {
    let pairing = a.start_outgoing_pairing(b_id).unwrap();
    let mut b_events = b.subscribe();
    let incoming = timeout(Duration::from_secs(2), async {
        loop {
            let event = b_events.recv().await.unwrap();
            if let myconnect::application::EventData::PairingRequested(snapshot) = event.event {
                return snapshot;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(incoming.verification_code, pairing.verification_code);
    b.accept_pairing(incoming.id).unwrap();
    wait_for_paired(a, b_id, true).await;
    wait_for_paired(b, a_id, true).await;
}

#[tokio::test]
async fn discovery_pairing_and_ping_succeed_over_encrypted_dispatch() {
    let a = peer("Peer A");
    let b = peer("Peer B");
    let a_id = a.identity.device_id().to_owned();
    let b_id = b.identity.device_id().to_owned();
    let a_udp = free_udp_addr();
    let b_udp = free_udp_addr();

    let a_service = LanService::start(
        test_config(a_udp, b_udp),
        local_with_ping(&a_id, "Peer A"),
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
        local_with_ping(&b_id, "Peer B"),
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

    // Before pairing, a ping must not be delivered even though both sides
    // are connected and capability-compatible.
    assert!(matches!(
        a.application.send_ping(&b_id, Some("too early".into())),
        Err(ApplicationError::NotPaired)
    ));

    pair(&a.application, &b.application, &a_id, &b_id).await;

    // Now that both devices are paired and each advertised
    // `kdeconnect.ping`, a ping sent over the TLS-protected connection must
    // be decoded and dispatched to the plugin handler on the other side.
    a.application
        .send_ping(&b_id, Some("hello from A".into()))
        .unwrap();
    wait_for_ping(&b.application, &a_id, "hello from A").await;

    // And the reverse direction, proving the dispatch path is symmetric.
    b.application
        .send_ping(&a_id, Some("hello from B".into()))
        .unwrap();
    wait_for_ping(&a.application, &b_id, "hello from B").await;

    a_service.shutdown().await.unwrap();
    b_service.shutdown().await.unwrap();
}

#[tokio::test]
async fn capability_filtering_prevents_sending_ping_to_a_peer_that_never_advertised_it() {
    let a = peer("Peer A");
    let b = peer("Peer B");
    let a_id = a.identity.device_id().to_owned();
    let b_id = b.identity.device_id().to_owned();
    let a_udp = free_udp_addr();
    let b_udp = free_udp_addr();

    let a_service = LanService::start(
        test_config(a_udp, b_udp),
        local_with_ping(&a_id, "Peer A"),
        a.application.clone(),
        a.commands,
        a.identity.clone(),
        a.trust_store.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    // B advertises no plugin capabilities at all.
    let b_service = LanService::start(
        test_config(b_udp, a_udp),
        local_without_capabilities(&b_id, "Peer B"),
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

    pair(&a.application, &b.application, &a_id, &b_id).await;

    // B is paired and connected, but never advertised `kdeconnect.ping` in
    // its incoming capabilities, so sending must be refused with a typed
    // rejection rather than silently dropped or delivered anyway.
    assert!(matches!(
        a.application
            .send_ping(&b_id, Some("should not send".into())),
        Err(ApplicationError::UnsupportedByPeer)
    ));
    assert_eq!(b.application.last_ping_received(&a_id), None);

    a_service.shutdown().await.unwrap();
    b_service.shutdown().await.unwrap();
}
