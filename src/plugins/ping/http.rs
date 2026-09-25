use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use serde::Deserialize;

use super::send_ping;
use crate::{api::ApiProblem, core::PluginContext};

pub(super) fn routes(ctx: PluginContext) -> Router {
    Router::new()
        .route("/devices/{device_id}/ping", post(post_ping))
        .with_state(ctx)
}

#[derive(Deserialize)]
struct PingRequest {
    message: Option<String>,
}

/// Queue a `kdeconnect.ping` to a paired, connected device. The JSON body
/// is optional; without one, a plain ping carrying no message is sent.
async fn post_ping(
    State(ctx): State<PluginContext>,
    Path(device_id): Path<String>,
    request: Option<Json<PingRequest>>,
) -> Result<StatusCode, ApiProblem> {
    let message = request.and_then(|Json(request)| request.message);
    send_ping(&ctx, &device_id, message)?;
    Ok(StatusCode::ACCEPTED)
}
