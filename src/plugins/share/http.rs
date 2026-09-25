use axum::{
    Extension, Json, Router,
    extract::{Multipart, Path, State},
    http::StatusCode,
    routing::post,
};

use super::send_file;
use crate::{
    api::{ApiProblem, UploadIdleTimeout, declared_size, forward_upload, next_field, skip_field},
    application::{PluginContext, TransferSnapshot},
};

pub(super) fn streaming_routes(ctx: PluginContext) -> Router {
    Router::new()
        .route("/devices/{device_id}/share", post(post_share))
        .with_state(ctx)
}

/// Send a file to a paired device, streamed from a `multipart/form-data`
/// upload with one `file` part, which must carry its length in a
/// `Content-Length` part header (the transfer's declared size). The file is
/// never buffered whole: each chunk is forwarded, as it arrives, to the
/// transfer's task and on to the device. `202` with the transfer once the
/// whole file has been forwarded; the request fails with `request_timeout`
/// only if the upload stalls.
async fn post_share(
    State(ctx): State<PluginContext>,
    Path(device_id): Path<String>,
    Extension(UploadIdleTimeout(idle)): Extension<UploadIdleTimeout>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<TransferSnapshot>), ApiProblem> {
    let mut created = None;
    while let Some(mut field) = next_field(idle, &mut multipart).await? {
        if field.name() != Some("file") {
            skip_field(idle, field).await?;
            continue;
        }
        let file_name = field.file_name().unwrap_or_default().to_owned();
        let (transfer, sender) = send_file(&ctx, &device_id, file_name, declared_size(&field)?)?;
        created = Some(transfer.id);
        forward_upload(idle, &mut field, sender).await?;
    }
    let transfer_id = created.ok_or_else(|| ApiProblem::bad_request("missing_file_part"))?;
    let latest = ctx
        .transfers()
        .get(transfer_id)
        .ok_or_else(ApiProblem::internal)?;
    Ok((StatusCode::ACCEPTED, Json(latest)))
}
