use std::{sync::Arc, time::Duration};

use myconnect::{
    api::{ApiServer, ApiServerConfig},
    application::{
        ApplicationHandle, ClipboardSnapshot, Command, EventData, LocalDeviceSnapshot,
        TransferConfig,
    },
    clipboard::InMemoryClipboard,
    config::{ApiToken, FilesystemTrustStore, LocalIdentity},
    device::DeviceRegistry,
    protocol::{DeviceType, IdentityBody},
};
use serde_json::Map;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};
use tokio_util::sync::CancellationToken;

struct TestServer {
    _directory: tempfile::TempDir,
    token: ApiToken,
    application: ApplicationHandle,
    commands: tokio::sync::mpsc::Receiver<Command>,
    server: ApiServer,
}

impl TestServer {
    async fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let token = ApiToken::load_or_create(directory.path()).unwrap();
        let identity =
            Arc::new(LocalIdentity::load_or_create(directory.path().join("identity")).unwrap());
        let (application, commands) = ApplicationHandle::new(
            LocalDeviceSnapshot {
                device_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                device_name: "Test Device".into(),
            },
            8,
            b"test-local-pubkey".to_vec(),
            Arc::new(FilesystemTrustStore::new(directory.path())),
            InMemoryClipboard::shared(),
            4,
            4,
            identity,
            TransferConfig::new(directory.path().join("downloads")),
        )
        .unwrap();
        let mut devices = DeviceRegistry::new();
        devices
            .discover(
                &IdentityBody {
                    device_id: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                    device_name: "Peer Phone".into(),
                    device_type: DeviceType::Phone,
                    incoming_capabilities: vec!["kdeconnect.clipboard".into()],
                    outgoing_capabilities: vec!["kdeconnect.share.request".into()],
                    protocol_version: 8,
                    extra: Map::new(),
                },
                false,
                10,
            )
            .unwrap();
        application.replace_devices(devices).unwrap();
        let server = ApiServer::start(
            ApiServerConfig::new(0)
                .unwrap()
                .with_shutdown_timeout(Duration::from_secs(2)),
            Arc::new(application.clone()),
            token.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert!(server.local_addr().ip().is_loopback());
        Self {
            _directory: directory,
            token,
            application,
            commands,
            server,
        }
    }

    fn authorization(&self) -> String {
        format!("Authorization: Bearer {}\r\n", self.token.expose_secret())
    }

    /// Register a live, paired connection for `device_id`, as the transport
    /// layer would after a real handshake, so transfer and ping endpoints
    /// have somewhere to send a `kdeconnect.share.request` or
    /// `kdeconnect.ping`.
    fn connect_and_pair(
        &self,
        device_id: &str,
    ) -> tokio::sync::mpsc::Receiver<myconnect::protocol::Packet> {
        let identity = IdentityBody {
            device_id: device_id.to_owned(),
            device_name: "Peer Phone".into(),
            device_type: DeviceType::Phone,
            incoming_capabilities: vec![
                "kdeconnect.share.request".into(),
                "kdeconnect.ping".into(),
            ],
            outgoing_capabilities: vec!["kdeconnect.share.request".into()],
            protocol_version: 8,
            extra: Map::new(),
        };
        self.application
            .discover_device(&identity, true, 20)
            .unwrap();
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        self.application
            .register_connection(
                device_id,
                vec![1, 2, 3],
                8,
                tx,
                CancellationToken::new(),
                20,
            )
            .unwrap();
        rx
    }
}

async fn request(server: &TestServer, method: &str, path: &str, authenticated: bool) -> String {
    let mut stream = TcpStream::connect(server.server.local_addr())
        .await
        .unwrap();
    let authorization = if authenticated {
        server.authorization()
    } else {
        String::new()
    };
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\n{authorization}Connection: close\r\nContent-Length: 0\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    String::from_utf8(response).unwrap()
}

async fn request_with_body(
    server: &TestServer,
    method: &str,
    path: &str,
    json_body: &str,
) -> String {
    let mut stream = TcpStream::connect(server.server.local_addr())
        .await
        .unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\n{}Content-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{json_body}",
        server.authorization(),
        json_body.len(),
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    String::from_utf8(response).unwrap()
}

fn body(response: &str) -> &str {
    response.split_once("\r\n\r\n").unwrap().1
}

#[tokio::test]
async fn control_plane_is_authenticated_and_runs_on_ephemeral_loopback() {
    let mut server = TestServer::start().await;

    let mut unauthorized_body = None;
    for (method, path) in [
        ("GET", "/api/v1/status"),
        ("POST", "/api/v1/discovery"),
        ("GET", "/api/v1/devices"),
        ("GET", "/api/v1/devices/missing"),
        ("GET", "/api/v1/events"),
    ] {
        let response = request(&server, method, path, false).await;
        assert!(response.starts_with("HTTP/1.1 401 Unauthorized"));
        assert!(
            response
                .to_ascii_lowercase()
                .contains("content-type: application/problem+json")
        );
        match &unauthorized_body {
            Some(expected) => assert_eq!(body(&response), expected),
            None => unauthorized_body = Some(body(&response).to_owned()),
        }
    }

    let status = request(&server, "GET", "/api/v1/status", true).await;
    assert!(status.starts_with("HTTP/1.1 200 OK"));
    assert!(status.to_ascii_lowercase().contains("x-request-id:"));
    assert!(
        !status
            .to_ascii_lowercase()
            .contains("access-control-allow-origin")
    );
    let status_json: serde_json::Value = serde_json::from_str(body(&status)).unwrap();
    assert_eq!(status_json["protocolVersion"], 8);
    assert_eq!(
        status_json["localDevice"]["deviceId"],
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );

    let devices = request(&server, "GET", "/api/v1/devices", true).await;
    let devices_json: serde_json::Value = serde_json::from_str(body(&devices)).unwrap();
    assert_eq!(devices_json.as_array().unwrap().len(), 1);
    assert_eq!(devices_json[0]["deviceName"], "Peer Phone");
    let device = request(
        &server,
        "GET",
        "/api/v1/devices/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        true,
    )
    .await;
    let device_json: serde_json::Value = serde_json::from_str(body(&device)).unwrap();
    assert_eq!(device_json["reachability"], "discovered");
    let missing = request(&server, "GET", "/api/v1/devices/missing", true).await;
    assert!(missing.starts_with("HTTP/1.1 404 Not Found"));
    assert!(body(&missing).contains("device_not_found"));
    let wrong_method = request(&server, "POST", "/api/v1/status", true).await;
    assert!(wrong_method.starts_with("HTTP/1.1 405 Method Not Allowed"));
    assert!(
        wrong_method
            .to_ascii_lowercase()
            .contains("content-type: application/problem+json")
    );

    let mut oversized_stream = TcpStream::connect(server.server.local_addr())
        .await
        .unwrap();
    let oversized_request = format!(
        "POST /api/v1/discovery HTTP/1.1\r\nHost: localhost\r\n{}Connection: close\r\nContent-Length: 65537\r\n\r\n",
        server.authorization()
    );
    oversized_stream
        .write_all(oversized_request.as_bytes())
        .await
        .unwrap();
    let mut oversized_response = Vec::new();
    timeout(
        Duration::from_secs(2),
        oversized_stream.read_to_end(&mut oversized_response),
    )
    .await
    .unwrap()
    .unwrap();
    let oversized_response = String::from_utf8(oversized_response).unwrap();
    assert!(oversized_response.starts_with("HTTP/1.1 413 Payload Too Large"));
    assert!(body(&oversized_response).contains("payload_too_large"));

    let discovery = request(&server, "POST", "/api/v1/discovery", true).await;
    assert!(discovery.starts_with("HTTP/1.1 202 Accepted"));
    assert_eq!(
        server.commands.recv().await,
        Some(Command::AnnounceDiscovery)
    );

    server.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn sse_delivers_typed_events_and_shutdown_cleans_up_clients() {
    let server = TestServer::start().await;
    let mut stream = TcpStream::connect(server.server.local_addr())
        .await
        .unwrap();
    let request = format!(
        "GET /api/v1/events HTTP/1.1\r\nHost: localhost\r\n{}Connection: keep-alive\r\n\r\n",
        server.authorization()
    );
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut response = Vec::new();
    timeout(Duration::from_secs(2), async {
        while !response.windows(4).any(|window| window == b"\r\n\r\n") {
            let mut buffer = [0_u8; 512];
            let read = stream.read(&mut buffer).await.unwrap();
            assert!(read > 0);
            response.extend_from_slice(&buffer[..read]);
        }
    })
    .await
    .unwrap();
    assert!(String::from_utf8_lossy(&response).starts_with("HTTP/1.1 200 OK"));

    server
        .application
        .event_bus()
        .publish(EventData::ClipboardChanged(ClipboardSnapshot {
            text: "event payload".into(),
            updated_at: 12,
            source_device_id: None,
        }))
        .unwrap();

    timeout(Duration::from_secs(2), async {
        loop {
            let mut buffer = [0_u8; 512];
            let read = stream.read(&mut buffer).await.unwrap();
            assert!(read > 0);
            response.extend_from_slice(&buffer[..read]);
            if response
                .windows(b"event: clipboard.changed".len())
                .any(|window| window == b"event: clipboard.changed")
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    let response = String::from_utf8_lossy(&response);
    assert!(response.contains("\"type\":\"clipboard.changed\""));
    assert!(response.contains("\"sequence\":1"));

    server.server.shutdown().await.unwrap();
    let mut remaining = Vec::new();
    timeout(Duration::from_secs(1), stream.read_to_end(&mut remaining))
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn clipboard_get_and_put_round_trip_and_enforce_the_size_limit() {
    let server = TestServer::start().await;

    let empty = request(&server, "GET", "/api/v1/clipboard", true).await;
    assert!(empty.starts_with("HTTP/1.1 200 OK"));
    let empty_json: serde_json::Value = serde_json::from_str(body(&empty)).unwrap();
    assert_eq!(empty_json["text"], "");

    let put = request_with_body(
        &server,
        "PUT",
        "/api/v1/clipboard",
        r#"{"text":"hello from api"}"#,
    )
    .await;
    assert!(put.starts_with("HTTP/1.1 200 OK"));
    let put_json: serde_json::Value = serde_json::from_str(body(&put)).unwrap();
    assert_eq!(put_json["text"], "hello from api");

    let get = request(&server, "GET", "/api/v1/clipboard", true).await;
    let get_json: serde_json::Value = serde_json::from_str(body(&get)).unwrap();
    assert_eq!(get_json["text"], "hello from api");

    // Oversized clipboard text is rejected with a typed, clipboard-specific
    // problem, distinct from the generic request-body-too-large rejection
    // (the API's default body limit is larger than the clipboard limit).
    let oversized = format!(r#"{{"text":"{}"}}"#, "x".repeat(40 * 1024));
    let oversized_response =
        request_with_body(&server, "PUT", "/api/v1/clipboard", &oversized).await;
    assert!(oversized_response.starts_with("HTTP/1.1 413 Payload Too Large"));
    assert!(body(&oversized_response).contains("clipboard_text_too_large"));

    server.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn ping_is_queued_to_a_paired_device_with_an_optional_message() {
    let server = TestServer::start().await;
    let device_id = "cccccccccccccccccccccccccccccccc";
    let mut packets = server.connect_and_pair(device_id);

    let with_message = request_with_body(
        &server,
        "POST",
        &format!("/api/v1/devices/{device_id}/ping"),
        r#"{"message":"hello from api"}"#,
    )
    .await;
    assert!(
        with_message.starts_with("HTTP/1.1 202 Accepted"),
        "unexpected response: {with_message}"
    );
    let sent = packets.try_recv().unwrap();
    assert_eq!(sent.packet_type, "kdeconnect.ping");
    assert_eq!(sent.body["message"], "hello from api");

    // Without a body, a plain ping carrying no message is sent.
    let plain = request(
        &server,
        "POST",
        &format!("/api/v1/devices/{device_id}/ping"),
        true,
    )
    .await;
    assert!(plain.starts_with("HTTP/1.1 202 Accepted"));
    let sent = packets.try_recv().unwrap();
    assert_eq!(sent.packet_type, "kdeconnect.ping");
    assert!(!sent.body.contains_key("message"));

    let unknown = request(
        &server,
        "POST",
        "/api/v1/devices/dddddddddddddddddddddddddddddddd/ping",
        true,
    )
    .await;
    assert!(unknown.starts_with("HTTP/1.1 404 Not Found"));
    assert!(body(&unknown).contains("device_not_found"));

    // The peer discovered at startup is not paired.
    let unpaired = request(
        &server,
        "POST",
        "/api/v1/devices/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/ping",
        true,
    )
    .await;
    assert!(unpaired.starts_with("HTTP/1.1 409 Conflict"));
    assert!(body(&unpaired).contains("device_not_paired"));

    server.server.shutdown().await.unwrap();
}

#[test]
fn kde_connect_ports_are_rejected() {
    for port in 1716..=1764 {
        assert!(ApiServerConfig::new(port).is_err());
    }
}

#[tokio::test]
async fn transfer_upload_is_streamed_queryable_and_cancellable() {
    let server = TestServer::start().await;
    let device_id = "cccccccccccccccccccccccccccccccc";
    let _packets = server.connect_and_pair(device_id);

    let boundary = "myconnect-test-boundary";
    let file_bytes = b"hello from the transfer test";
    let mut multipart_body = Vec::new();
    multipart_body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"deviceId\"\r\n\r\n{device_id}\r\n")
            .as_bytes(),
    );
    multipart_body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"hello.txt\"\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
            file_bytes.len()
        )
        .as_bytes(),
    );
    multipart_body.extend_from_slice(file_bytes);
    multipart_body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let mut stream = TcpStream::connect(server.server.local_addr())
        .await
        .unwrap();
    let http_request = format!(
        "POST /api/v1/transfers HTTP/1.1\r\nHost: localhost\r\n{}Content-Type: multipart/form-data; boundary={boundary}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        server.authorization(),
        multipart_body.len(),
    );
    stream.write_all(http_request.as_bytes()).await.unwrap();
    stream.write_all(&multipart_body).await.unwrap();
    let mut response = Vec::new();
    timeout(Duration::from_secs(5), stream.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    let response = String::from_utf8(response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 202 Accepted"),
        "unexpected response: {response}"
    );
    let created: serde_json::Value = serde_json::from_str(body(&response)).unwrap();
    assert_eq!(created["deviceId"], device_id);
    assert_eq!(created["fileName"], "hello.txt");
    assert_eq!(created["totalBytes"], file_bytes.len());
    assert_eq!(created["direction"], "outgoing");
    let transfer_id = created["id"].as_str().unwrap().to_owned();

    let listed = request(&server, "GET", "/api/v1/transfers", true).await;
    assert!(listed.starts_with("HTTP/1.1 200 OK"));
    let listed_json: serde_json::Value = serde_json::from_str(body(&listed)).unwrap();
    assert!(
        listed_json
            .as_array()
            .unwrap()
            .iter()
            .any(|transfer| transfer["id"] == transfer_id)
    );

    let fetched = request(
        &server,
        "GET",
        &format!("/api/v1/transfers/{transfer_id}"),
        true,
    )
    .await;
    assert!(fetched.starts_with("HTTP/1.1 200 OK"));

    let missing = request(
        &server,
        "GET",
        "/api/v1/transfers/00000000-0000-0000-0000-000000000000",
        true,
    )
    .await;
    assert!(missing.starts_with("HTTP/1.1 404 Not Found"));
    assert!(body(&missing).contains("transfer_not_found"));

    let cancelled = request(
        &server,
        "DELETE",
        &format!("/api/v1/transfers/{transfer_id}"),
        true,
    )
    .await;
    assert!(cancelled.starts_with("HTTP/1.1 200 OK"));
    let cancelled_json: serde_json::Value = serde_json::from_str(body(&cancelled)).unwrap();
    assert_eq!(cancelled_json["id"], transfer_id);

    server.server.shutdown().await.unwrap();
    server
        .application
        .shutdown_transfers(Duration::from_secs(1))
        .await;
}

#[tokio::test]
async fn transfer_upload_to_unpaired_device_is_rejected() {
    let server = TestServer::start().await;
    // `bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb` is discovered but neither paired
    // nor connected in `TestServer::start`.
    let device_id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let boundary = "myconnect-test-boundary";
    let file_bytes = b"should not be sent";
    let mut multipart_body = Vec::new();
    multipart_body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"deviceId\"\r\n\r\n{device_id}\r\n")
            .as_bytes(),
    );
    multipart_body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"secret.txt\"\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
            file_bytes.len()
        )
        .as_bytes(),
    );
    multipart_body.extend_from_slice(file_bytes);
    multipart_body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let mut stream = TcpStream::connect(server.server.local_addr())
        .await
        .unwrap();
    let http_request = format!(
        "POST /api/v1/transfers HTTP/1.1\r\nHost: localhost\r\n{}Content-Type: multipart/form-data; boundary={boundary}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        server.authorization(),
        multipart_body.len(),
    );
    stream.write_all(http_request.as_bytes()).await.unwrap();
    stream.write_all(&multipart_body).await.unwrap();
    let mut response = Vec::new();
    timeout(Duration::from_secs(5), stream.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    let response = String::from_utf8(response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 409 Conflict"),
        "unexpected response: {response}"
    );
    assert!(body(&response).contains("device_not_paired"));

    server.server.shutdown().await.unwrap();
}
