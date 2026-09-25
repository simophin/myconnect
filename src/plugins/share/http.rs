use axum::{
    Extension, Json, Router,
    extract::{Multipart, Path, Query, State, rejection::QueryRejection},
    http::StatusCode,
    routing::post,
};

use super::send_file;
use crate::{
    api::{
        ApiProblem, Forwarded, TransferQuery, UploadIdleTimeout, declared_size, forward_upload,
        next_field, requested_transfer_id, skip_field,
    },
    core::{PluginContext, TransferSnapshot},
};

pub(super) fn streaming_routes(ctx: PluginContext) -> Router {
    Router::new()
        .route("/devices/{device_id}/share", post(post_share))
        .with_state(ctx)
}

/// Send a file to a paired device, as a transfer with the id in
/// `?transferId=` if the client chose one, streamed from a
/// `multipart/form-data` upload with one `file` part, which must carry its length in a
/// `Content-Length` part header (the transfer's declared size). The file is
/// never buffered whole: each chunk is forwarded, as it arrives, to the
/// transfer's task and on to the device. `202` with the transfer once the
/// whole file has been forwarded, or at once if the transfer ends first
/// (e.g. it is cancelled), without reading the rest of the upload; the
/// request fails with `request_timeout` only if the upload stalls.
async fn post_share(
    State(ctx): State<PluginContext>,
    Path(device_id): Path<String>,
    query: Result<Query<TransferQuery>, QueryRejection>,
    Extension(UploadIdleTimeout(idle)): Extension<UploadIdleTimeout>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<TransferSnapshot>), ApiProblem> {
    let id = requested_transfer_id(query)?;
    let mut created = None;
    while let Some(mut field) = next_field(idle, &mut multipart).await? {
        if field.name() != Some("file") {
            skip_field(idle, field).await?;
            continue;
        }
        let file_name = field.file_name().unwrap_or_default().to_owned();
        let (transfer, sender) =
            send_file(&ctx, &device_id, file_name, declared_size(&field)?, id)?;
        created = Some(transfer.id);
        if forward_upload(idle, &mut field, sender).await? == Forwarded::TransferEnded {
            // Cancelled or failed: answer with how it ended rather than
            // read the rest of the upload.
            break;
        }
    }
    let transfer_id = created.ok_or_else(|| ApiProblem::bad_request("missing_file_part"))?;
    let latest = ctx
        .transfers()
        .get(transfer_id)
        .ok_or_else(ApiProblem::internal)?;
    Ok((StatusCode::ACCEPTED, Json(latest)))
}
