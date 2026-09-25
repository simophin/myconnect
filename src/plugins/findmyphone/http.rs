use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};

use super::ring_device;
use crate::{api::ApiProblem, core::PluginContext};

pub(super) fn routes(ctx: PluginContext) -> Router {
    Router::new()
        .route("/devices/{device_id}/ring", post(post_ring))
        .with_state(ctx)
}

/// Ask a paired, connected device that advertises
/// `kdeconnect.findmyphone.request` to ring so it can be found.
async fn post_ring(
    State(ctx): State<PluginContext>,
    Path(device_id): Path<String>,
) -> Result<StatusCode, ApiProblem> {
    ring_device(&ctx, &device_id)?;
    Ok(StatusCode::ACCEPTED)
}
