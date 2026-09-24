use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{Path, State},
    http::{
        HeaderMap, Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use futures_util::StreamExt;
use myconnect::{
    application::{
        ApplicationEvent, ClipboardSnapshot, EventData, PairingDirection, PairingSnapshot,
        PairingStatus, TransferDirection, TransferSnapshot, TransferStatus,
    },
    client::{
        ApiClient, ClientError, ClipboardWatchUpdate, DeviceWatchUpdate, TransferWatchUpdate,
    },
    config::ApiToken,
    device::{DeviceReachability, DeviceSnapshot},
    protocol::DeviceType,
};
use serde::Deserialize;
use serde_json::json;
use tempfile::TempDir;
use tokio::{net::TcpListener, task::JoinHandle, time::timeout};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[derive(Clone, Default)]
struct MockState {
    device_reads: Arc<AtomicUsize>,
    transfer_reads: Arc<AtomicUsize>,
    clipboard_reads: Arc<AtomicUsize>,
}

struct MockServer {
    url: String,
    state: MockState,
    task: JoinHandle<()>,
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl MockServer {
    async fn start() -> Self {
        let state = MockState::default();
        let app = Router::new()
            .route("/api/v1/devices", get(devices))
            .route("/api/v1/devices/{device_id}", delete(unpair))
            .route("/api/v1/devices/{device_id}/ping", post(ping))
            .route("/api/v1/pairings", post(start_pairing))
            .route(
                "/api/v1/pairings/{pairing_id}",
                get(pairing).delete(reject_pairing),
            )
            .route("/api/v1/pairings/{pairing_id}/accept", post(accept_pairing))
            .route("/api/v1/transfers", post(start_transfer))
            .route("/api/v1/transfers/{transfer_id}", get(transfer))
            .route("/api/v1/clipboard", get(clipboard).put(set_clipboard))
            .route("/api/v1/events", get(events))
            .layer(middleware::from_fn(authorize))
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            url: format!("http://{address}"),
            state,
            task,
        }
    }

    fn client(&self) -> ApiClient {
        ApiClient::new(&self.url, Some(ApiToken::from_secret(TOKEN).unwrap())).unwrap()
    }
}

async fn authorize(request: Request<Body>, next: Next) -> Response {
    if request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        == Some(&format!("Bearer {TOKEN}"))
    {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"code": "unauthorized"})),
        )
            .into_response()
    }
}

fn device() -> DeviceSnapshot {
    DeviceSnapshot {
        device_id: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        device_name: "Peer Phone".into(),
        device_type: DeviceType::Phone,
        protocol_version: 8,
        incoming_capabilities: vec!["kdeconnect.clipboard".into()],
        outgoing_capabilities: vec!["kdeconnect.share.request".into()],
        reachability: DeviceReachability::Connected,
        paired: true,
        pairing: false,
        last_seen_at: 10,
        battery: None,
    }
}

fn pairing_snapshot(status: PairingStatus) -> PairingSnapshot {
    PairingSnapshot {
        id: Uuid::from_u128(1),
        device_id: device().device_id,
        device_name: "Peer Phone".into(),
        direction: PairingDirection::Outgoing,
        status,
        verification_code: Some("ABCDEF12".into()),
        created_at: 10,
        expires_at: 40,
        error_code: None,
    }
}

fn transfer_snapshot() -> TransferSnapshot {
    TransferSnapshot {
        id: Uuid::from_u128(2),
        device_id: device().device_id,
        device_name: "Peer Phone".into(),
        direction: TransferDirection::Outgoing,
        status: TransferStatus::Transferring,
        file_name: "payload.txt".into(),
        total_bytes: 12,
        transferred_bytes: 3,
        created_at: 10,
        updated_at: 11,
        error_code: None,
        saved_path: None,
    }
}

async fn devices(State(state): State<MockState>) -> Json<Vec<DeviceSnapshot>> {
    state.device_reads.fetch_add(1, Ordering::SeqCst);
    Json(vec![device()])
}

async fn unpair(Path(device_id): Path<String>) -> Response {
    if device_id == device().device_id {
        StatusCode::NO_CONTENT.into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "device_not_found"})),
        )
            .into_response()
    }
}

#[derive(Deserialize)]
struct Ping {
    message: Option<String>,
}

async fn ping(Path(device_id): Path<String>, Json(request): Json<Ping>) -> Response {
    if device_id != device().device_id {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "device_not_found"})),
        )
            .into_response();
    }
    if request.message.as_deref() == Some("unsupported") {
        return (
            StatusCode::CONFLICT,
            Json(json!({"code": "unsupported_by_peer"})),
        )
            .into_response();
    }
    StatusCode::ACCEPTED.into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartPairing {
    device_id: String,
}

async fn start_pairing(Json(request): Json<StartPairing>) -> (StatusCode, Json<PairingSnapshot>) {
    assert_eq!(request.device_id, device().device_id);
    (
        StatusCode::ACCEPTED,
        Json(pairing_snapshot(PairingStatus::Requested)),
    )
}

async fn pairing(Path(pairing_id): Path<Uuid>) -> Response {
    if pairing_id == Uuid::max() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "pairing_not_found"})),
        )
            .into_response();
    }
    Json(pairing_snapshot(PairingStatus::AwaitingConfirmation)).into_response()
}

async fn accept_pairing(Path(pairing_id): Path<Uuid>) -> Json<PairingSnapshot> {
    assert_eq!(pairing_id, Uuid::from_u128(1));
    Json(pairing_snapshot(PairingStatus::Accepted))
}

async fn reject_pairing(Path(pairing_id): Path<Uuid>) -> StatusCode {
    assert_eq!(pairing_id, Uuid::from_u128(1));
    StatusCode::NO_CONTENT
}

async fn start_transfer(headers: HeaderMap, body: Bytes) -> (StatusCode, Json<TransferSnapshot>) {
    assert!(
        headers[CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("multipart/form-data; boundary=")
    );
    assert!(
        body.windows(b"streamed body".len())
            .any(|window| window == b"streamed body")
    );
    // The daemon rejects a file part without its own Content-Length header.
    let part_length = format!("content-length: {}\r\n", b"streamed body".len());
    assert!(
        body.to_ascii_lowercase()
            .windows(part_length.len())
            .any(|window| window == part_length.as_bytes())
    );
    assert!(
        body.windows(device().device_id.len())
            .any(|window| window == device().device_id.as_bytes())
    );
    (StatusCode::ACCEPTED, Json(transfer_snapshot()))
}

async fn transfer(State(state): State<MockState>, Path(transfer_id): Path<Uuid>) -> Response {
    state.transfer_reads.fetch_add(1, Ordering::SeqCst);
    if transfer_id == Uuid::max() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "transfer_not_found"})),
        )
            .into_response();
    }
    Json(transfer_snapshot()).into_response()
}

async fn clipboard(State(state): State<MockState>) -> Json<ClipboardSnapshot> {
    state.clipboard_reads.fetch_add(1, Ordering::SeqCst);
    Json(ClipboardSnapshot {
        text: "current".into(),
        updated_at: 10,
        source_device_id: None,
    })
}

#[derive(Deserialize)]
struct SetClipboard {
    text: String,
}

async fn set_clipboard(Json(request): Json<SetClipboard>) -> Response {
    if request.text == "fail" {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"code": "clipboard_unavailable"})),
        )
            .into_response();
    }
    Json(ClipboardSnapshot {
        text: request.text,
        updated_at: 11,
        source_device_id: None,
    })
    .into_response()
}

async fn events() -> Response {
    let event = ApplicationEvent {
        sequence: 1,
        timestamp: 12,
        event: EventData::ClipboardChanged(ClipboardSnapshot {
            text: "changed".into(),
            updated_at: 12,
            source_device_id: None,
        }),
    };
    let body = format!(
        "event: clipboard.changed\ndata: {}\n\n",
        serde_json::to_string(&event).unwrap()
    );
    ([(CONTENT_TYPE, "text/event-stream")], body).into_response()
}

#[tokio::test]
async fn every_client_operation_uses_the_expected_http_contract() {
    let server = MockServer::start().await;
    let client = server.client();
    assert_eq!(client.devices().await.unwrap(), vec![device()]);
    assert_eq!(
        client
            .start_pairing(&device().device_id)
            .await
            .unwrap()
            .status,
        PairingStatus::Requested
    );
    assert_eq!(
        client.pairing(Uuid::from_u128(1)).await.unwrap().status,
        PairingStatus::AwaitingConfirmation
    );
    assert_eq!(
        client
            .accept_pairing(Uuid::from_u128(1))
            .await
            .unwrap()
            .status,
        PairingStatus::Accepted
    );
    client.reject_pairing(Uuid::from_u128(1)).await.unwrap();
    client.unpair(&device().device_id).await.unwrap();
    client.ping(&device().device_id, None).await.unwrap();
    client
        .ping(&device().device_id, Some("hello"))
        .await
        .unwrap();

    let directory = TempDir::new().unwrap();
    let file = directory.path().join("payload.txt");
    tokio::fs::write(&file, b"streamed body").await.unwrap();
    assert_eq!(
        client
            .send_file(&device().device_id, &file)
            .await
            .unwrap()
            .id,
        Uuid::from_u128(2)
    );
    assert_eq!(
        client.transfer(Uuid::from_u128(2)).await.unwrap().status,
        TransferStatus::Transferring
    );
    assert_eq!(client.clipboard().await.unwrap().text, "current");
    assert_eq!(
        client.set_clipboard("updated").await.unwrap().text,
        "updated"
    );
    let mut events = client.events().await.unwrap();
    assert!(matches!(
        events.next().await.unwrap().unwrap().event,
        EventData::ClipboardChanged(_)
    ));
}

#[tokio::test]
async fn watch_modes_refetch_snapshots_after_event_stream_disconnects() {
    let server = MockServer::start().await;
    let client = server.client();

    let cancellation = CancellationToken::new();
    let stop = cancellation.clone();
    let state = server.state.clone();
    timeout(
        Duration::from_secs(2),
        client.watch_devices(cancellation, move |update| {
            if matches!(update, DeviceWatchUpdate::Snapshot(_))
                && state.device_reads.load(Ordering::SeqCst) >= 2
            {
                stop.cancel();
            }
        }),
    )
    .await
    .unwrap()
    .unwrap();

    let cancellation = CancellationToken::new();
    let stop = cancellation.clone();
    let state = server.state.clone();
    timeout(
        Duration::from_secs(2),
        client.watch_clipboard(cancellation, move |update| {
            if matches!(update, ClipboardWatchUpdate::Snapshot(_))
                && state.clipboard_reads.load(Ordering::SeqCst) >= 2
            {
                stop.cancel();
            }
        }),
    )
    .await
    .unwrap()
    .unwrap();

    let cancellation = CancellationToken::new();
    let stop = cancellation.clone();
    let state = server.state.clone();
    timeout(
        Duration::from_secs(2),
        client.watch_transfer(Uuid::from_u128(2), cancellation, move |update| {
            if matches!(update, TransferWatchUpdate::Snapshot(_))
                && state.transfer_reads.load(Ordering::SeqCst) >= 2
            {
                stop.cancel();
            }
        }),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test]
async fn errors_are_distinct_and_actionable() {
    let server = MockServer::start().await;
    let wrong = ApiClient::new(
        &server.url,
        Some(
            ApiToken::from_secret(
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            )
            .unwrap(),
        ),
    )
    .unwrap();
    assert!(matches!(
        wrong.devices().await,
        Err(ClientError::Unauthorized)
    ));
    let anonymous = ApiClient::new(&server.url, None).unwrap();
    assert!(matches!(
        anonymous.devices().await,
        Err(ClientError::Unauthorized)
    ));

    let client = server.client();
    assert!(matches!(
        client.pairing(Uuid::max()).await,
        Err(ClientError::NotFound("pairing"))
    ));
    assert!(matches!(
        client.unpair("missing").await,
        Err(ClientError::NotFound("device"))
    ));
    assert!(matches!(
        client.ping("missing", None).await,
        Err(ClientError::NotFound("device"))
    ));
    assert!(matches!(
        client.ping(&device().device_id, Some("unsupported")).await,
        Err(ClientError::OperationFailed { status: 409, .. })
    ));
    assert!(matches!(
        client.set_clipboard("fail").await,
        Err(ClientError::OperationFailed { status: 503, .. })
    ));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let absent = ApiClient::new(
        &format!("http://{address}"),
        Some(ApiToken::from_secret(TOKEN).unwrap()),
    )
    .unwrap();
    assert!(matches!(
        absent.devices().await,
        Err(ClientError::DaemonUnavailable)
    ));
}

#[test]
fn client_allows_non_loopback_hosts() {
    assert!(
        ApiClient::new(
            "http://example.com",
            Some(ApiToken::from_secret(TOKEN).unwrap())
        )
        .is_ok()
    );
}

#[test]
fn client_rejects_non_http_schemes() {
    assert!(matches!(
        ApiClient::new(
            "https://127.0.0.1",
            Some(ApiToken::from_secret(TOKEN).unwrap())
        ),
        Err(ClientError::UnsupportedScheme)
    ));
}
