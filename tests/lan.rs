use std::{
    collections::HashSet,
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};

use myconnect::{
    application::{
        ApplicationHandle, ApplicationService, Command, EventData, LocalDeviceSnapshot, Query,
        QueryResult,
    },
    device::DeviceReachability,
    protocol::{DeviceType, IdentityBody, Packet, PacketCodec},
    transport::lan::{
        LanConfig, LanService, LocalDeviceInfo, MAX_DISCOVERY_DATAGRAM, TCP_PORT_RANGE,
    },
};
use serde_json::{Map, json};
use tokio::{net::UdpSocket, sync::mpsc, time::timeout};
use tokio_util::sync::CancellationToken;

fn local(device_id: &str, name: &str) -> LocalDeviceInfo {
    LocalDeviceInfo {
        device_id: device_id.into(),
        device_name: name.into(),
        device_type: DeviceType::Desktop,
        incoming_capabilities: vec!["kdeconnect.ping".into()],
        outgoing_capabilities: vec!["kdeconnect.ping".into()],
    }
}

fn application(device_id: &str, name: &str) -> (ApplicationHandle, mpsc::Receiver<Command>) {
    ApplicationHandle::new(
        LocalDeviceSnapshot {
            device_id: device_id.into(),
            device_name: name.into(),
        },
        8,
        32,
        128,
    )
    .unwrap()
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
    let a_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let b_id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let a_udp = free_udp_addr();
    let b_udp = free_udp_addr();
    let (a_app, a_commands) = application(a_id, "Peer A");
    let (b_app, b_commands) = application(b_id, "Peer B");
    let mut b_events = b_app.subscribe();
    let a_shutdown = CancellationToken::new();
    let b_shutdown = CancellationToken::new();
    let a = LanService::start(
        test_config(a_udp, b_udp),
        local(a_id, "Peer A"),
        a_app.clone(),
        a_commands,
        HashSet::new(),
        a_shutdown,
    )
    .await
    .unwrap();
    let b = LanService::start(
        test_config(b_udp, a_udp),
        local(b_id, "Peer B"),
        b_app.clone(),
        b_commands,
        HashSet::new(),
        b_shutdown,
    )
    .await
    .unwrap();
    assert!(TCP_PORT_RANGE.contains(&a.tcp_addr().port()));
    assert!(TCP_PORT_RANGE.contains(&b.tcp_addr().port()));
    assert_ne!(a.tcp_addr().port(), b.tcp_addr().port());

    wait_for_reachability(&a_app, b_id, DeviceReachability::Connected).await;
    wait_for_reachability(&b_app, a_id, DeviceReachability::Connected).await;
    for _ in 0..10 {
        a_app.command(Command::AnnounceDiscovery).unwrap();
        b_app.command(Command::AnnounceDiscovery).unwrap();
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(matches!(
        a_app.query(Query::Devices).unwrap(),
        QueryResult::Devices(devices) if devices.len() == 1
    ));
    assert!(matches!(
        b_app.query(Query::Devices).unwrap(),
        QueryResult::Devices(devices) if devices.len() == 1
    ));

    a.shutdown().await.unwrap();
    wait_for_reachability(&b_app, a_id, DeviceReachability::Unavailable).await;

    let new_a_udp = free_udp_addr();
    let (new_a_app, new_a_commands) = application(a_id, "Peer A");
    let new_a = LanService::start(
        test_config(new_a_udp, b.discovery_addr()),
        local(a_id, "Peer A"),
        new_a_app,
        new_a_commands,
        HashSet::new(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    wait_for_reachability(&b_app, a_id, DeviceReachability::Connected).await;

    let mut connected_events = 0;
    while let Ok(event) = b_events.try_recv() {
        if matches!(event.event, EventData::DeviceConnected(_)) {
            connected_events += 1;
        }
    }
    assert_eq!(connected_events, 2, "one connection per peer address");

    new_a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[tokio::test]
async fn malformed_oversized_self_and_unsupported_discovery_are_ignored() {
    let local_id = "cccccccccccccccccccccccccccccccc";
    let bind = free_udp_addr();
    let (application, commands) = application(local_id, "Local");
    let service = LanService::start(
        test_config(bind, bind).with_announcement_targets(Vec::new()),
        local(local_id, "Local"),
        application.clone(),
        commands,
        HashSet::new(),
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
        .send_to(&identity_packet(local_id, 8), service.discovery_addr())
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
        application.query(Query::Devices).unwrap(),
        QueryResult::Devices(Vec::new())
    );
    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn service_can_restart_without_leaking_sockets_or_tasks() {
    for index in 0..3 {
        let id = format!("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{index}");
        let (application, commands) = application(&id, "Restart");
        let service = LanService::start(
            test_config(free_udp_addr(), free_udp_addr()).with_announcement_targets(Vec::new()),
            local(&id, "Restart"),
            application,
            commands,
            HashSet::new(),
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
