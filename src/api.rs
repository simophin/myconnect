//! Authenticated, loopback-only HTTP control plane.

use std::{
    convert::Infallible,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use axum::{
    Json, Router,
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
use serde::Serialize;
use thiserror::Error;
use tokio::{net::TcpListener, task::JoinHandle, time::timeout};
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};

use crate::{
    application::{
        ApplicationError, ApplicationEvent, ApplicationService, Command, Query, QueryResult,
        StatusSnapshot,
    },
    config::ApiToken,
    device::DeviceSnapshot,
};

pub const DEFAULT_API_PORT: u16 = 24_816;
const REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Clone, Debug)]
pub struct ApiServerConfig {
    port: u16,
    request_timeout: Duration,
    shutdown_timeout: Duration,
    max_request_body_bytes: usize,
}

impl ApiServerConfig {
    pub fn new(port: u16) -> Result<Self, ApiServerError> {
        if (1716..=1764).contains(&port) {
            return Err(ApiServerError::ReservedKdeConnectPort(port));
        }
        Ok(Self {
            port,
            request_timeout: Duration::from_secs(15),
            shutdown_timeout: Duration::from_secs(5),
            max_request_body_bytes: 64 * 1024,
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))
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
        token: ApiToken,
        shutdown: CancellationToken,
    ) -> Result<Self, ApiServerError> {
        let listener = TcpListener::bind(config.bind_addr())
            .await
            .map_err(ApiServerError::Bind)?;
        let local_addr = listener.local_addr().map_err(ApiServerError::Bind)?;
        debug_assert!(local_addr.ip().is_loopback());

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

fn router(state: ApiState, token: ApiToken, config: &ApiServerConfig) -> Router {
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

    let api = Router::new()
        .route("/status", get(get_status))
        .route("/discovery", post(post_discovery))
        .route("/devices", get(get_devices))
        .route("/devices/{device_id}", get(get_device))
        .route("/events", get(get_events))
        .fallback(api_not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(middleware::from_fn_with_state(
            config.max_request_body_bytes,
            enforce_content_length,
        ))
        .layer(middleware::from_fn_with_state(
            config.request_timeout,
            enforce_request_timeout,
        ))
        .layer(middleware::from_fn_with_state(
            Arc::new(token),
            require_authentication,
        ))
        .with_state(state);

    Router::new()
        .nest("/api/v1", api)
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(config.max_request_body_bytes))
        .layer(middleware)
}

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

async fn post_discovery(State(state): State<ApiState>) -> Result<StatusCode, ApiProblem> {
    state
        .application
        .command(Command::AnnounceDiscovery)
        .map_err(map_error)?;
    Ok(StatusCode::ACCEPTED)
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

fn sse_event(event: &ApplicationEvent) -> Option<Event> {
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

fn map_error(error: ApplicationError) -> ApiProblem {
    match error {
        ApplicationError::CommandQueueFull => ApiProblem::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Service unavailable",
            "command_queue_full",
        ),
        ApplicationError::CommandQueueClosed => ApiProblem::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Service unavailable",
            "application_unavailable",
        ),
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
}

struct ApiProblem {
    status: StatusCode,
    body: ProblemBody,
}

impl ApiProblem {
    fn new(status: StatusCode, title: &'static str, code: &'static str) -> Self {
        Self {
            status,
            body: ProblemBody {
                problem_type: "about:blank",
                title,
                status: status.as_u16(),
                code,
            },
        }
    }

    fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "Unauthorized", "unauthorized")
    }

    fn not_found(code: &'static str) -> Self {
        Self::new(StatusCode::NOT_FOUND, "Not found", code)
    }

    fn internal() -> Self {
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
