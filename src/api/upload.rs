//! Helpers for streaming routes: uploads forwarded chunk by chunk rather
//! than buffered, each step bounded by an idle timeout instead of a
//! deadline for the whole request.

use std::{future::Future, time::Duration};

use axum::{
    extract::{Multipart, multipart::Field},
    http::{StatusCode, header::CONTENT_LENGTH},
};
use bytes::Bytes;
use tokio::{sync::mpsc, time::timeout};

use super::ApiProblem;

/// How long a streaming route's upload may go without making progress. The
/// server adds it to every request on a streaming route, as an extension;
/// a handler takes it with `Extension<UploadIdleTimeout>`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct UploadIdleTimeout(pub(crate) Duration);

/// Await one step of an upload, failing with `request_timeout` if it makes
/// no progress within `limit`. Returning early drops the upload's chunk
/// sender, which fails the transfer rather than leaving it hanging.
async fn idle_timeout<T>(limit: Duration, step: impl Future<Output = T>) -> Result<T, ApiProblem> {
    timeout(limit, step).await.map_err(|_| {
        ApiProblem::new(
            StatusCode::REQUEST_TIMEOUT,
            "Request timeout",
            "request_timeout",
        )
    })
}

/// The next part of a multipart upload, if there is one.
pub(crate) async fn next_field(
    idle: Duration,
    multipart: &mut Multipart,
) -> Result<Option<Field<'_>>, ApiProblem> {
    idle_timeout(idle, multipart.next_field())
        .await?
        .map_err(|_| ApiProblem::bad_request("invalid_multipart"))
}

/// A text part's value.
pub(crate) async fn field_text(idle: Duration, field: Field<'_>) -> Result<String, ApiProblem> {
    idle_timeout(idle, field.text())
        .await?
        .map_err(|_| ApiProblem::bad_request("invalid_multipart"))
}

/// Read past a part the route doesn't use.
pub(crate) async fn skip_field(idle: Duration, field: Field<'_>) -> Result<(), ApiProblem> {
    let _ = idle_timeout(idle, field.bytes()).await?;
    Ok(())
}

/// A file part's size, from its own `Content-Length` header, which a
/// streamed upload must declare up front.
pub(crate) fn declared_size(field: &Field<'_>) -> Result<u64, ApiProblem> {
    field
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| ApiProblem::bad_request("missing_declared_size"))
}

/// Forward a file part into a transfer's chunk channel as it arrives.
/// Dropping the sender at the end tells the transfer the upload is over.
pub(crate) async fn forward_upload(
    idle: Duration,
    field: &mut Field<'_>,
    sender: mpsc::Sender<Bytes>,
) -> Result<(), ApiProblem> {
    loop {
        let chunk = idle_timeout(idle, field.chunk())
            .await?
            .map_err(|_| ApiProblem::bad_request("invalid_multipart"))?;
        let Some(chunk) = chunk else { break };
        if chunk.is_empty() {
            continue;
        }
        // The receiver is dropped once the transfer's task ends (success,
        // failure, or cancellation); there is nothing more useful to do
        // with the rest of the upload then, so stop forwarding it.
        if idle_timeout(idle, sender.send(chunk)).await?.is_err() {
            break;
        }
    }
    Ok(())
}
