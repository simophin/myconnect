//! HTTP control plane, bound to loopback by default. Requests must carry a
//! bearer token only when the server was started with one.

use std::{
    convert::Infallible,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, State},
    http::{
        HeaderName, HeaderValue, Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, WWW_AUTHENTICATE},
    },
    middleware::{self, Next},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{net::TcpListener, task::JoinHandle, time::timeout};
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use uuid::Uuid;

mod upload;

pub(crate) use upload::{
    UploadIdleTimeout, declared_size, field_text, forward_upload, next_field, skip_field,
};

use crate::{
    config::ApiToken,
    core::{
        ApplicationService, Command, CoreError, CoreEvent, DEFAULT_MAX_TRANSFER_BYTES,
        PairingSnapshot, Query, QueryResult, SettingsPatch, SettingsSnapshot, StatusSnapshot,
        TransferSnapshot,
    },
    device::DeviceSnapshot,
};

pub const DEFAULT_API_PORT: u16 = 24_816;
const REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Clone, Debug)]
pub struct ApiServerConfig {
    host: IpAddr,
    port: u16,
    request_timeout: Duration,
    shutdown_timeout: Duration,
    max_request_body_bytes: usize,
    max_transfer_body_bytes: usize,
}

impl ApiServerConfig {
    pub fn new(port: u16) -> Result<Self, ApiServerError> {
        if (1716..=1764).contains(&port) {
            return Err(ApiServerError::ReservedKdeConnectPort(port));
        }
        Ok(Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port,
            request_timeout: Duration::from_secs(15),
            shutdown_timeout: Duration::from_secs(5),
            max_request_body_bytes: 64 * 1024,
            // Large enough for the configured maximum transfer size plus a
            // small allowance for multipart boundaries and headers; the
            // upload itself is streamed rather than buffered, so this is a
            // sanity ceiling rather than a memory budget.
            max_transfer_body_bytes: DEFAULT_MAX_TRANSFER_BYTES.saturating_add(64 * 1024) as usize,
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    pub fn with_host(mut self, value: IpAddr) -> Self {
        self.host = value;
        self
    }

    pub fn with_request_timeout(mut self, value: Duration) -> Self {
        self.request_timeout = value;
        self
    }

    pub fn with_shutdown_timeout(mut self, value: Duration) -> Self {
        self.shutdown_timeout = value;
        self
    }

    pub fn with_max_request_body_bytes(mut self, value: usize) -> Self {
        self.max_request_body_bytes = value;
        self
    }

    pub fn with_max_transfer_body_bytes(mut self, value: usize) -> Self {
        self.max_transfer_body_bytes = value;
        self
    }
}

impl Default for ApiServerConfig {
    fn default() -> Self {
        Self::new(DEFAULT_API_PORT).expect("default API port is outside KDE Connect's range")
    }
}

#[derive(Clone)]
struct ApiState {
    application: Arc<dyn ApplicationService>,
    shutdown: CancellationToken,
}

pub struct ApiServer {
    local_addr: SocketAddr,
    shutdown: CancellationToken,
    shutdown_timeout: Duration,
    task: Option<JoinHandle<std::io::Result<()>>>,
}

impl ApiServer {
    pub async fn start(
        config: ApiServerConfig,
        application: Arc<dyn ApplicationService>,
        token: Option<ApiToken>,
        shutdown: CancellationToken,
    ) -> Result<Self, ApiServerError> {
        let listener = TcpListener::bind(config.bind_addr())
            .await
            .map_err(ApiServerError::Bind)?;
        let local_addr = listener.local_addr().map_err(ApiServerError::Bind)?;

        let router = router(
            ApiState {
                application,
                shutdown: shutdown.clone(),
            },
            token,
            &config,
        );
        let serve_shutdown = shutdown.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(serve_shutdown.cancelled_owned())
                .await
        });

        Ok(Self {
            local_addr,
            shutdown,
            shutdown_timeout: config.shutdown_timeout,
            task: Some(task),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub async fn shutdown(mut self) -> Result<(), ApiServerError> {
        self.shutdown.cancel();
        let mut task = self.task.take().expect("server task is present");
        match timeout(self.shutdown_timeout, &mut task).await {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(error))) => Err(ApiServerError::Serve(error)),
            Ok(Err(error)) => Err(ApiServerError::Task(error)),
            Err(_) => {
                task.abort();
                let _ = task.await;
                Err(ApiServerError::ShutdownDeadline)
            }
        }
    }
}

impl Drop for ApiServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

fn router(state: ApiState, token: Option<ApiToken>, config: &ApiServerConfig) -> Router {
    let request_id_header = HeaderName::from_static(REQUEST_ID_HEADER);
    let middleware = ServiceBuilder::new()
        .layer(SetRequestIdLayer::new(
            request_id_header.clone(),
            MakeRequestUuid,
        ))
        .layer(PropagateRequestIdLayer::new(request_id_header))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().include_headers(false))
                .on_response(DefaultOnResponse::new().include_headers(false)),
        );

    // Streaming routes take uploads: the body is forwarded as it arrives
    // rather than buffered, so they get the transfer-sized body limit, and
    // no deadline for the whole request, since a large file legitimately
    // takes longer than that; handlers bound each step of the upload by the
    // idle timeout instead. Layers apply to the routes a router has when
    // they are added, so each set keeps its own limits after the merge.
    let streaming = state
        .application
        .plugin_streaming_routes()
        .layer(Extension(UploadIdleTimeout(config.request_timeout)))
        .layer(DefaultBodyLimit::max(config.max_transfer_body_bytes))
        .layer(middleware::from_fn_with_state(
            config.max_transfer_body_bytes,
            enforce_content_length,
        ));

    let plugin_routes = state.application.plugin_routes();
    let api = Router::new()
        .route("/status", get(get_status))
        .route("/discovery", post(post_discovery))
        .route("/devices", get(get_devices))
        .route(
            "/devices/{device_id}",
            get(get_device).delete(delete_device),
        )
        .route("/pairings", get(get_pairings).post(post_pairing))
        .route(
            "/pairings/{pairing_id}",
            get(get_pairing).delete(delete_pairing),
        )
        .route("/pairings/{pairing_id}/accept", post(post_pairing_accept))
        .route("/transfers", get(get_transfers))
        .route(
            "/transfers/{transfer_id}",
            get(get_transfer).delete(delete_transfer),
        )
        .route("/settings", get(get_settings).patch(patch_settings))
        .route("/events", get(get_events))
        .with_state(state)
        // Plugins' routes get the same deadline, limits and authentication
        // as the core's.
        .merge(plugin_routes)
        .layer(middleware::from_fn_with_state(
            config.request_timeout,
            enforce_request_timeout,
        ))
        .layer(middleware::from_fn_with_state(
            config.max_request_body_bytes,
            enforce_content_length,
        ))
        .merge(streaming)
        .fallback(api_not_found)
        .method_not_allowed_fallback(method_not_allowed);
    // Without a configured token the API is open to any local client, which
    // is the default for a CLI-started daemon bound to loopback.
    let api = match token {
        Some(token) => api.layer(middleware::from_fn_with_state(
            Arc::new(token),
            require_authentication,
        )),
        None => api,
    };

    Router::new()
        .nest("/api/v1", api)
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(config.max_request_body_bytes))
        .layer(middleware)
}

/// Refuse a request whose declared length is over `maximum` before reading
/// any of it.
async fn enforce_content_length(
    State(maximum): State<usize>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let too_large = request
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|length| length > maximum);
    if too_large {
        ApiProblem::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Payload too large",
            "payload_too_large",
        )
        .into_response()
    } else {
        next.run(request).await
    }
}

async fn enforce_request_timeout(
    State(deadline): State<Duration>,
    request: Request<Body>,
    next: Next,
) -> Response {
    match timeout(deadline, next.run(request)).await {
        Ok(response) => response,
        Err(_) => ApiProblem::new(
            StatusCode::REQUEST_TIMEOUT,
            "Request timeout",
            "request_timeout",
        )
        .into_response(),
    }
}

async fn require_authentication(
    State(token): State<Arc<ApiToken>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let authorized = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|candidate| token.constant_time_matches(candidate));

    if authorized {
        next.run(request).await
    } else {
        let mut response = ApiProblem::unauthorized().into_response();
        response.headers_mut().insert(
            WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer realm=\"myconnect\""),
        );
        response
    }
}

async fn get_status(State(state): State<ApiState>) -> Result<Json<StatusSnapshot>, ApiProblem> {
    match state.application.query(Query::Status).map_err(map_error)? {
        QueryResult::Status(status) => Ok(Json(status)),
        _ => Err(ApiProblem::internal()),
    }
}

async fn get_devices(
    State(state): State<ApiState>,
) -> Result<Json<Vec<DeviceSnapshot>>, ApiProblem> {
    match state.application.query(Query::Devices).map_err(map_error)? {
        QueryResult::Devices(devices) => Ok(Json(devices)),
        _ => Err(ApiProblem::internal()),
    }
}

async fn get_device(
    State(state): State<ApiState>,
    Path(device_id): Path<String>,
) -> Result<Json<DeviceSnapshot>, ApiProblem> {
    match state
        .application
        .query(Query::Device { device_id })
        .map_err(map_error)?
    {
        QueryResult::Device(Some(device)) => Ok(Json(device)),
        QueryResult::Device(None) => Err(ApiProblem::not_found("device_not_found")),
        _ => Err(ApiProblem::internal()),
    }
}

#[derive(Deserialize)]
struct DiscoveryRequest {
    address: Option<String>,
}

/// Announce this device so peers answer promptly. Without a body (or
/// without `address`) the announcement is broadcast; with an IPv4 address,
/// it is sent to that address only, for networks where broadcast doesn't
/// reach the peer.
async fn post_discovery(
    State(state): State<ApiState>,
    request: Option<Json<DiscoveryRequest>>,
) -> Result<StatusCode, ApiProblem> {
    match request.and_then(|Json(request)| request.address) {
        Some(address) => {
            let address = address
                .trim()
                .parse()
                .map_err(|_| ApiProblem::bad_request("invalid_address"))?;
            state.application.announce_to(address).map_err(map_error)?;
        }
        None => state
            .application
            .command(Command::AnnounceDiscovery)
            .map_err(map_error)?,
    }
    Ok(StatusCode::ACCEPTED)
}

async fn delete_device(
    State(state): State<ApiState>,
    Path(device_id): Path<String>,
) -> Result<StatusCode, ApiProblem> {
    state
        .application
        .forget_device(&device_id)
        .map_err(map_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartPairingRequest {
    device_id: String,
}

async fn post_pairing(
    State(state): State<ApiState>,
    Json(request): Json<StartPairingRequest>,
) -> Result<(StatusCode, Json<PairingSnapshot>), ApiProblem> {
    let pairing = state
        .application
        .start_outgoing_pairing(&request.device_id)
        .map_err(map_error)?;
    Ok((StatusCode::ACCEPTED, Json(pairing)))
}

/// Every pairing this daemon process knows about, including terminal ones,
/// so a client that (re)connects can find incoming requests still awaiting
/// confirmation without having observed their `pairing.requested` event.
async fn get_pairings(
    State(state): State<ApiState>,
) -> Result<Json<Vec<PairingSnapshot>>, ApiProblem> {
    match state
        .application
        .query(Query::Pairings)
        .map_err(map_error)?
    {
        QueryResult::Pairings(pairings) => Ok(Json(pairings)),
        _ => Err(ApiProblem::internal()),
    }
}

async fn get_pairing(
    State(state): State<ApiState>,
    Path(pairing_id): Path<Uuid>,
) -> Result<Json<PairingSnapshot>, ApiProblem> {
    match state
        .application
        .query(Query::Pairing { pairing_id })
        .map_err(map_error)?
    {
        QueryResult::Pairing(Some(pairing)) => Ok(Json(pairing)),
        QueryResult::Pairing(None) => Err(ApiProblem::not_found("pairing_not_found")),
        _ => Err(ApiProblem::internal()),
    }
}

async fn post_pairing_accept(
    State(state): State<ApiState>,
    Path(pairing_id): Path<Uuid>,
) -> Result<Json<PairingSnapshot>, ApiProblem> {
    let pairing = state
        .application
        .accept_pairing(pairing_id)
        .map_err(map_error)?;
    Ok(Json(pairing))
}

async fn delete_pairing(
    State(state): State<ApiState>,
    Path(pairing_id): Path<Uuid>,
) -> Result<Json<PairingSnapshot>, ApiProblem> {
    let pairing = state
        .application
        .cancel_pairing(pairing_id)
        .map_err(map_error)?;
    Ok(Json(pairing))
}

async fn get_transfers(
    State(state): State<ApiState>,
) -> Result<Json<Vec<TransferSnapshot>>, ApiProblem> {
    match state
        .application
        .query(Query::Transfers)
        .map_err(map_error)?
    {
        QueryResult::Transfers(transfers) => Ok(Json(transfers)),
        _ => Err(ApiProblem::internal()),
    }
}

async fn get_transfer(
    State(state): State<ApiState>,
    Path(transfer_id): Path<Uuid>,
) -> Result<Json<TransferSnapshot>, ApiProblem> {
    match state
        .application
        .query(Query::Transfer { transfer_id })
        .map_err(map_error)?
    {
        QueryResult::Transfer(Some(transfer)) => Ok(Json(transfer)),
        QueryResult::Transfer(None) => Err(ApiProblem::not_found("transfer_not_found")),
        _ => Err(ApiProblem::internal()),
    }
}

async fn delete_transfer(
    State(state): State<ApiState>,
    Path(transfer_id): Path<Uuid>,
) -> Result<Json<TransferSnapshot>, ApiProblem> {
    let transfer = state
        .application
        .cancel_transfer(transfer_id)
        .map_err(map_error)?;
    Ok(Json(transfer))
}

async fn get_settings(State(state): State<ApiState>) -> Result<Json<SettingsSnapshot>, ApiProblem> {
    match state
        .application
        .query(Query::Settings)
        .map_err(map_error)?
    {
        QueryResult::Settings(settings) => Ok(Json(settings)),
        _ => Err(ApiProblem::internal()),
    }
}

/// Change the fields present in the body; `null` resets one to its default.
/// The change is saved before it takes effect, and `settings.changed` is
/// published if anything changed.
async fn patch_settings(
    State(state): State<ApiState>,
    Json(patch): Json<SettingsPatch>,
) -> Result<Json<SettingsSnapshot>, ApiProblem> {
    let settings = state
        .application
        .update_settings(patch)
        .map_err(map_error)?;
    Ok(Json(settings))
}

async fn get_events(
    State(state): State<ApiState>,
) -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    let mut receiver = state.application.subscribe();
    let shutdown = state.shutdown;
    let stream = async_stream::stream! {
        loop {
            let received = tokio::select! {
                _ = shutdown.cancelled() => break,
                received = receiver.recv() => received,
            };
            match received {
                Ok(application_event) => {
                    let Some(event) = sse_event(&application_event) else { break };
                    yield Ok(event);
                }
                // A gap requires the client to reconnect and fetch snapshots.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
                | Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    )
}

fn sse_event(event: &CoreEvent) -> Option<Event> {
    let data = serde_json::to_string(event).ok()?;
    Some(
        Event::default()
            .id(event.sequence.to_string())
            .event(event.event.event_type())
            .data(data),
    )
}

async fn api_not_found() -> ApiProblem {
    ApiProblem::not_found("endpoint_not_found")
}

async fn method_not_allowed() -> ApiProblem {
    ApiProblem::new(
        StatusCode::METHOD_NOT_ALLOWED,
        "Method not allowed",
        "method_not_allowed",
    )
}

async fn not_found() -> ApiProblem {
    ApiProblem::not_found("not_found")
}

fn map_error(error: CoreError) -> ApiProblem {
    match error {
        CoreError::CommandQueueFull => ApiProblem::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Service unavailable",
            "command_queue_full",
        ),
        CoreError::CommandQueueClosed => ApiProblem::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Service unavailable",
            "application_unavailable",
        ),
        CoreError::UnknownDevice => ApiProblem::not_found("device_not_found"),
        CoreError::InvalidDiscoveryAddress => ApiProblem::bad_request("invalid_address"),
        CoreError::UnknownPairing => ApiProblem::not_found("pairing_not_found"),
        CoreError::AlreadyPaired => {
            ApiProblem::new(StatusCode::CONFLICT, "Conflict", "already_paired")
        }
        CoreError::PairingInProgress => {
            ApiProblem::new(StatusCode::CONFLICT, "Conflict", "pairing_in_progress")
        }
        CoreError::DeviceNotConnected => {
            ApiProblem::new(StatusCode::CONFLICT, "Conflict", "device_not_connected")
        }
        CoreError::InvalidPairingDirection => ApiProblem::new(
            StatusCode::CONFLICT,
            "Conflict",
            "invalid_pairing_direction",
        ),
        CoreError::InvalidPairingState | CoreError::InvalidTransition(_) => {
            ApiProblem::new(StatusCode::CONFLICT, "Conflict", "invalid_pairing_state")
        }
        CoreError::NotPaired => {
            ApiProblem::new(StatusCode::CONFLICT, "Conflict", "device_not_paired")
        }
        CoreError::UnsupportedByPeer => {
            ApiProblem::new(StatusCode::CONFLICT, "Conflict", "unsupported_by_peer")
        }
        CoreError::InvalidFileName => ApiProblem::bad_request("invalid_file_name"),
        CoreError::TransferTooLarge { .. } => ApiProblem::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Payload too large",
            "transfer_too_large",
        ),
        CoreError::UnknownTransfer => ApiProblem::not_found("transfer_not_found"),
        CoreError::InvalidDeviceName => ApiProblem::bad_request("invalid_device_name"),
        CoreError::InvalidDownloadDir => ApiProblem::bad_request("invalid_download_dir"),
        CoreError::InvalidSettings => ApiProblem::bad_request("invalid_settings"),
        CoreError::InvalidTransferState => {
            ApiProblem::new(StatusCode::CONFLICT, "Conflict", "invalid_transfer_state")
        }
        _ => ApiProblem::internal(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProblemBody {
    #[serde(rename = "type")]
    problem_type: &'static str,
    title: &'static str,
    status: u16,
    code: &'static str,
    /// Human-readable detail from the peer, when it gave one.
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

/// An `application/problem+json` error response. Plugin handlers return it
/// too, converting core errors with `?`.
pub(crate) struct ApiProblem {
    status: StatusCode,
    body: ProblemBody,
}

impl From<CoreError> for ApiProblem {
    fn from(error: CoreError) -> Self {
        map_error(error)
    }
}

impl ApiProblem {
    pub(crate) fn new(status: StatusCode, title: &'static str, code: &'static str) -> Self {
        Self {
            status,
            body: ProblemBody {
                problem_type: "about:blank",
                title,
                status: status.as_u16(),
                code,
                detail: None,
            },
        }
    }

    /// Add human-readable detail, e.g. an error message from the device.
    pub(crate) fn with_detail(mut self, detail: Option<String>) -> Self {
        self.body.detail = detail;
        self
    }

    fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "Unauthorized", "unauthorized")
    }

    pub(crate) fn bad_request(code: &'static str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "Bad request", code)
    }

    pub(crate) fn not_found(code: &'static str) -> Self {
        Self::new(StatusCode::NOT_FOUND, "Not found", code)
    }

    pub(crate) fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error",
            "internal_error",
        )
    }
}

impl IntoResponse for ApiProblem {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(self.body)).into_response();
        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

#[derive(Debug, Error)]
pub enum ApiServerError {
    #[error("port {0} is reserved for KDE Connect peer traffic")]
    ReservedKdeConnectPort(u16),
    #[error("local API listener could not be bound")]
    Bind(#[source] std::io::Error),
    #[error("local API server failed")]
    Serve(#[source] std::io::Error),
    #[error("local API server task failed")]
    Task(#[source] tokio::task::JoinError),
    #[error("local API server did not shut down before its deadline")]
    ShutdownDeadline,
}
