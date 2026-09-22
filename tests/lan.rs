use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use myconnect::{
    application::{
        ApplicationHandle, ApplicationService, Command, EventData, LocalDeviceSnapshot, Query,
        QueryResult,
    },
    clipboard::InMemoryClipboard,
    config::{FilesystemTrustStore, LocalIdentity, TrustStore},
    device::DeviceReachability,
    protocol::{DeviceType, IdentityBody, Packet, PacketCodec},
    transport::{
        lan::{LanConfig, LanService, LocalDeviceInfo, MAX_DISCOVERY_DATAGRAM, TCP_PORT_RANGE},
        tls::subject_public_key_info,
    },
};
use serde_json::{Map, json};
use tokio::{net::UdpSocket, sync::mpsc, time::timeout};
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
        incoming_capabilities: vec!["kdeconnect.ping".into()],
        outgoing_capabilities: vec!["kdeconnect.ping".into()],
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

#[tokio::test]
async fn two_peers_discover_connect_deduplicate_and_follow_address_changes() {
    let a = peer("Peer A");
    let b = peer("Peer B");
    let a_id = a.identity.device_id().to_owned();
    let b_id = b.identity.device_id().to_owned();
    let a_udp = free_udp_addr();
    let b_udp = free_udp_addr();
    let mut b_events = b.application.subscribe();
    let a_shutdown = CancellationToken::new();
    let b_shutdown = CancellationToken::new();
    let a_service = LanService::start(
        test_config(a_udp, b_udp),
        local(&a_id, "Peer A"),
        a.application.clone(),
        a.commands,
        a.identity.clone(),
        a.trust_store.clone(),
        a_shutdown,
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
        b_shutdown,
    )
    .await
    .unwrap();
    assert!(TCP_PORT_RANGE.contains(&a_service.tcp_addr().port()));
    assert!(TCP_PORT_RANGE.contains(&b_service.tcp_addr().port()));
    assert_ne!(a_service.tcp_addr().port(), b_service.tcp_addr().port());

    wait_for_reachability(&a.application, &b_id, DeviceReachability::Connected).await;
    wait_for_reachability(&b.application, &a_id, DeviceReachability::Connected).await;
    for _ in 0..10 {
        a.application.command(Command::AnnounceDiscovery).unwrap();
        b.application.command(Command::AnnounceDiscovery).unwrap();
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(matches!(
        a.application.query(Query::Devices).unwrap(),
        QueryResult::Devices(devices) if devices.len() == 1
    ));
    assert!(matches!(
        b.application.query(Query::Devices).unwrap(),
        QueryResult::Devices(devices) if devices.len() == 1
    ));

    a_service.shutdown().await.unwrap();
    wait_for_reachability(&b.application, &a_id, DeviceReachability::Unavailable).await;

    let new_a_udp = free_udp_addr();
    let new_a = peer("Peer A");
    // Re-use the original device ID's identity directory so the restarted
    // peer keeps its certificate (as the real daemon would across restarts).
    let new_a_service = LanService::start(
        test_config(new_a_udp, b_service.discovery_addr()),
        local(&a_id, "Peer A"),
        new_a.application,
        new_a.commands,
        a.identity.clone(),
        a.trust_store.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    wait_for_reachability(&b.application, &a_id, DeviceReachability::Connected).await;

    let mut connected_events = 0;
    while let Ok(event) = b_events.try_recv() {
        if matches!(event.event, EventData::DeviceConnected(_)) {
            connected_events += 1;
        }
    }
    assert_eq!(connected_events, 2, "one connection per peer address");

    new_a_service.shutdown().await.unwrap();
    b_service.shutdown().await.unwrap();
}

#[tokio::test]
async fn malformed_oversized_self_and_unsupported_discovery_are_ignored() {
    let local_peer = peer("Local");
    let local_id = local_peer.identity.device_id().to_owned();
    let bind = free_udp_addr();
    let service = LanService::start(
        test_config(bind, bind).with_announcement_targets(Vec::new()),
        local(&local_id, "Local"),
        local_peer.application.clone(),
        local_peer.commands,
        local_peer.identity,
        local_peer.trust_store,
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    sender
        .send_to(b"{not json}\n", service.discovery_addr())
        .await
        .unwrap();
    sender
        .send_to(
            &vec![b'x'; MAX_DISCOVERY_DATAGRAM + 1],
            service.discovery_addr(),
        )
        .await
        .unwrap();
    sender
        .send_to(&identity_packet(&local_id, 8), service.discovery_addr())
        .await
        .unwrap();
    sender
        .send_to(
            &identity_packet("dddddddddddddddddddddddddddddddd", 7),
            service.discovery_addr(),
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;

    assert_eq!(
        local_peer.application.query(Query::Devices).unwrap(),
        QueryResult::Devices(Vec::new())
    );
    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn service_can_restart_without_leaking_sockets_or_tasks() {
    for _ in 0..3 {
        let restart_peer = peer("Restart");
        let id = restart_peer.identity.device_id().to_owned();
        let service = LanService::start(
            test_config(free_udp_addr(), free_udp_addr()).with_announcement_targets(Vec::new()),
            local(&id, "Restart"),
            restart_peer.application,
            restart_peer.commands,
            restart_peer.identity,
            restart_peer.trust_store,
            CancellationToken::new(),
        )
        .await
        .unwrap();
        service.shutdown().await.unwrap();
    }
}

fn identity_packet(device_id: &str, protocol_version: u8) -> Vec<u8> {
    let identity = IdentityBody {
        device_id: device_id.into(),
        device_name: "Test Peer".into(),
        device_type: DeviceType::Desktop,
        incoming_capabilities: Vec::new(),
        outgoing_capabilities: Vec::new(),
        protocol_version,
        extra: Map::from_iter([("tcpPort".into(), json!(1716))]),
    };
    let packet = Packet::from_body(0, "kdeconnect.identity", &identity).unwrap();
    PacketCodec::new(MAX_DISCOVERY_DATAGRAM)
        .encode(&packet)
        .unwrap()
}
