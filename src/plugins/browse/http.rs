use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{
        HeaderValue, StatusCode,
        header::{CONTENT_LENGTH, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use tokio_util::io::ReaderStream;

use super::{BrowseError, BrowsePlugin, DirectoryListing, FileEntry};
use crate::{
    api::{
        ApiProblem, UploadIdleTimeout, declared_size, field_text, forward_upload, next_field,
        skip_field,
    },
    core::{PluginContext, TransferSnapshot},
};

#[derive(Clone)]
struct BrowseState {
    plugin: Arc<BrowsePlugin>,
    ctx: PluginContext,
}

pub(super) fn routes(plugin: Arc<BrowsePlugin>, ctx: PluginContext) -> Router {
    Router::new()
        .route(
            "/devices/{device_id}/files",
            get(get_files).delete(delete_file),
        )
        .route("/devices/{device_id}/files/content", get(get_file_content))
        .route(
            "/devices/{device_id}/files/download",
            post(post_file_download),
        )
        .route(
            "/devices/{device_id}/files/directories",
            post(post_directory),
        )
        .route("/devices/{device_id}/files/move", post(post_file_move))
        .with_state(BrowseState { plugin, ctx })
}

pub(super) fn streaming_routes(plugin: Arc<BrowsePlugin>, ctx: PluginContext) -> Router {
    Router::new()
        .route("/devices/{device_id}/files/upload", post(post_file_upload))
        .with_state(BrowseState { plugin, ctx })
}

impl From<BrowseError> for ApiProblem {
    fn from(error: BrowseError) -> Self {
        match error {
            BrowseError::Core(error) => error.into(),
            BrowseError::InvalidPath => ApiProblem::bad_request("invalid_path"),
            BrowseError::Unavailable { reason } => {
                ApiProblem::new(StatusCode::CONFLICT, "Conflict", "files_unavailable")
                    .with_detail(reason)
            }
            BrowseError::NotFound => ApiProblem::not_found("file_not_found"),
            BrowseError::Exists => ApiProblem::new(StatusCode::CONFLICT, "Conflict", "file_exists"),
            BrowseError::PermissionDenied => {
                ApiProblem::new(StatusCode::FORBIDDEN, "Forbidden", "file_permission_denied")
            }
            BrowseError::NotADirectory => {
                ApiProblem::new(StatusCode::CONFLICT, "Conflict", "not_a_directory")
            }
            BrowseError::IsADirectory => {
                ApiProblem::new(StatusCode::CONFLICT, "Conflict", "is_a_directory")
            }
            BrowseError::HostKeyMismatch => ApiProblem::new(
                StatusCode::BAD_GATEWAY,
                "Bad gateway",
                "files_host_key_mismatch",
            ),
            BrowseError::Failed => {
                ApiProblem::new(StatusCode::BAD_GATEWAY, "Bad gateway", "files_failed")
            }
            BrowseError::TimedOut => ApiProblem::new(
                StatusCode::GATEWAY_TIMEOUT,
                "Gateway timeout",
                "files_timed_out",
            ),
        }
    }
}

#[derive(Deserialize)]
struct FilePathQuery {
    path: Option<String>,
}

impl FilePathQuery {
    fn required(self) -> Result<String, ApiProblem> {
        self.path
            .ok_or_else(|| ApiProblem::bad_request("missing_path"))
    }
}

#[derive(Deserialize)]
struct FilePathRequest {
    path: String,
}

#[derive(Deserialize)]
struct MoveRequest {
    from: String,
    to: String,
}

/// List a directory on a paired device, or, without `path`, the storage
/// roots it shares. The first request opens a browse session with the
/// device, which can take a few seconds.
async fn get_files(
    State(state): State<BrowseState>,
    Path(device_id): Path<String>,
    Query(query): Query<FilePathQuery>,
) -> Result<Json<DirectoryListing>, ApiProblem> {
    let listing = state
        .plugin
        .list_files(&state.ctx, &device_id, query.path.as_deref())
        .await?;
    Ok(Json(listing))
}

/// Stream a file's content from a paired device, e.g. for a preview. To keep
/// a copy, `POST .../files/download` saves it as a transfer instead.
async fn get_file_content(
    State(state): State<BrowseState>,
    Path(device_id): Path<String>,
    Query(query): Query<FilePathQuery>,
) -> Result<Response, ApiProblem> {
    let content = state
        .plugin
        .open_file(&state.ctx, &device_id, &query.required()?)
        .await?;
    let content_type = content_type_for(&content.name);
    let size = content.size;
    let body = Body::from_stream(ReaderStream::new(content));
    Ok((
        [
            (CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (CONTENT_LENGTH, HeaderValue::from(size)),
        ],
        body,
    )
        .into_response())
}

/// A media type for common previewable files, by extension.
fn content_type_for(name: &str) -> &'static str {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    match extension.as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("heic") => "image/heic",
        Some("txt" | "log" | "md") => "text/plain; charset=utf-8",
        Some("pdf") => "application/pdf",
        Some("mp4") => "video/mp4",
        Some("mp3") => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

/// Save a file from a paired device into the download directory. The copy
/// runs as an incoming transfer; `202` once it has started.
async fn post_file_download(
    State(state): State<BrowseState>,
    Path(device_id): Path<String>,
    Json(request): Json<FilePathRequest>,
) -> Result<(StatusCode, Json<TransferSnapshot>), ApiProblem> {
    let transfer = state
        .plugin
        .download(&state.ctx, &device_id, &request.path)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(transfer)))
}

/// Stream a `multipart/form-data` upload (a `path` text field naming the
/// directory on the device, then one `file` part with a `Content-Length`
/// header) into a directory on a paired device, as an outgoing transfer.
/// A name that is taken gets a ` (n)` suffix. Like `POST
/// /devices/{id}/share`, the response comes once the whole file has been
/// forwarded, and the request fails only if the upload stalls.
async fn post_file_upload(
    State(state): State<BrowseState>,
    Path(device_id): Path<String>,
    Extension(UploadIdleTimeout(idle)): Extension<UploadIdleTimeout>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<TransferSnapshot>), ApiProblem> {
    let mut directory: Option<String> = None;
    let mut created = None;

    while let Some(mut field) = next_field(idle, &mut multipart).await? {
        match field.name() {
            Some("path") => directory = Some(field_text(idle, field).await?),
            Some("file") => {
                let directory = directory
                    .clone()
                    .ok_or_else(|| ApiProblem::bad_request("missing_path"))?;
                let file_name = field.file_name().unwrap_or_default().to_owned();
                let declared_size = declared_size(&field)?;

                // Opening a browse session can take longer than the idle
                // timeout allows, but it is bounded by its own timeouts.
                let (transfer, sender) = state
                    .plugin
                    .upload(
                        &state.ctx,
                        &device_id,
                        &directory,
                        &file_name,
                        declared_size,
                    )
                    .await?;
                created = Some(transfer.id);
                forward_upload(idle, &mut field, sender).await?;
            }
            _ => skip_field(idle, field).await?,
        }
    }

    let transfer_id = created.ok_or_else(|| ApiProblem::bad_request("missing_file_part"))?;
    let latest = state
        .ctx
        .transfers()
        .get(transfer_id)
        .ok_or_else(ApiProblem::internal)?;
    Ok((StatusCode::ACCEPTED, Json(latest)))
}

/// Create a directory on a paired device; `201` with its entry.
async fn post_directory(
    State(state): State<BrowseState>,
    Path(device_id): Path<String>,
    Json(request): Json<FilePathRequest>,
) -> Result<(StatusCode, Json<FileEntry>), ApiProblem> {
    let entry = state
        .plugin
        .create_directory(&state.ctx, &device_id, &request.path)
        .await?;
    Ok((StatusCode::CREATED, Json(entry)))
}

/// Move or rename a file or directory on a paired device. Fails with
/// `file_exists` rather than replacing anything.
async fn post_file_move(
    State(state): State<BrowseState>,
    Path(device_id): Path<String>,
    Json(request): Json<MoveRequest>,
) -> Result<Json<FileEntry>, ApiProblem> {
    let entry = state
        .plugin
        .move_file(&state.ctx, &device_id, &request.from, &request.to)
        .await?;
    Ok(Json(entry))
}

/// Delete a file, or a directory and everything in it, on a paired device.
async fn delete_file(
    State(state): State<BrowseState>,
    Path(device_id): Path<String>,
    Query(query): Query<FilePathQuery>,
) -> Result<StatusCode, ApiProblem> {
    state
        .plugin
        .delete(&state.ctx, &device_id, &query.required()?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
