//! Helpers for streaming routes: uploads forwarded chunk by chunk rather
//! than buffered, each step bounded by an idle timeout instead of a
//! deadline for the whole request.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, ready},
    time::Duration,
};

use axum::{
    body::{Body, BodyDataStream},
    extract::{Multipart, Query, Request, multipart::Field, rejection::QueryRejection},
    http::{StatusCode, header::CONTENT_LENGTH},
};
use bytes::Bytes;
use futures_core::Stream;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::{runtime::Handle, sync::mpsc, time::timeout};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::ApiProblem;

/// How long a streaming route's upload may go without making progress. The
/// server adds it to every request on a streaming route, as an extension;
/// a handler takes it with `Extension<UploadIdleTimeout>`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct UploadIdleTimeout(pub(crate) Duration);

/// The query of a streaming route that starts a transfer.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferQuery {
    transfer_id: Option<Uuid>,
}

/// The id the client chose for the transfer (`?transferId=`), if it did,
/// so it can follow the transfer, and stop sending once it ends, before
/// the route answers. `invalid_transfer_id` if it isn't a UUID.
pub(crate) fn requested_transfer_id(
    query: Result<Query<TransferQuery>, QueryRejection>,
) -> Result<Option<Uuid>, ApiProblem> {
    query
        .map(|Query(query)| query.transfer_id)
        .map_err(|_| ApiProblem::bad_request("invalid_transfer_id"))
}

/// Let a streaming route answer before it has read the whole upload (the
/// transfer was cancelled, or the request is refused) without cutting the
/// client off. HTTP/1.1 has no way to refuse the rest of a body: closing
/// the connection with unread data resets it, and a client still sending
/// would get a broken pipe instead of the answer. So if the handler drops
/// the body unfinished, the rest is read and thrown away on a task of its
/// own while the answer goes out, as web servers do ("lingering close").
/// The drain stops at the end of the body, on an error, once the client
/// has sent nothing for `idle`, or at `shutdown`, which it mustn't hold up.
pub(crate) fn linger_after_answer(
    idle: Duration,
    shutdown: CancellationToken,
    request: Request,
) -> Request {
    request.map(|body| {
        Body::from_stream(Lingering {
            stream: Some(body.into_data_stream()),
            idle,
            shutdown,
            ended: false,
        })
    })
}

/// A request body that drains itself if dropped before its end; see
/// [`linger_after_answer`].
struct Lingering {
    stream: Option<BodyDataStream>,
    idle: Duration,
    shutdown: CancellationToken,
    ended: bool,
}

impl Stream for Lingering {
    type Item = Result<Bytes, axum::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let Some(stream) = self.stream.as_mut() else {
            return Poll::Ready(None);
        };
        let item = ready!(stream.poll_next_unpin(cx));
        if !matches!(item, Some(Ok(_))) {
            self.ended = true;
        }
        Poll::Ready(item)
    }
}

impl Drop for Lingering {
    fn drop(&mut self) {
        if self.ended {
            return;
        }
        let (Some(mut stream), Ok(runtime)) = (self.stream.take(), Handle::try_current()) else {
            return;
        };
        let idle = self.idle;
        let shutdown = self.shutdown.clone();
        runtime.spawn(async move {
            let drain = async { while let Ok(Some(Ok(_))) = timeout(idle, stream.next()).await {} };
            tokio::select! {
                () = shutdown.cancelled() => {}
                () = drain => {}
            }
        });
    }
}

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

/// How forwarding a file part into a transfer ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Forwarded {
    /// The whole part went into the transfer.
    All,
    /// The transfer ended first: it was cancelled or failed. The rest of
    /// the upload is of no use then, so the route should answer at once
    /// instead of reading on, which for a large file would outlast the idle
    /// timeout.
    TransferEnded,
}

/// Forward a file part into a transfer's chunk channel as it arrives,
/// until the part ends or the transfer does. Dropping the sender at the
/// end tells the transfer the upload is over.
pub(crate) async fn forward_upload(
    idle: Duration,
    field: &mut Field<'_>,
    sender: mpsc::Sender<Bytes>,
) -> Result<Forwarded, ApiProblem> {
    loop {
        // The receiver is dropped once the transfer's task ends (success,
        // failure, or cancellation), which may come while the client is
        // slow to send the next chunk.
        let chunk = tokio::select! {
            () = sender.closed() => return Ok(Forwarded::TransferEnded),
            chunk = idle_timeout(idle, field.chunk()) => chunk?,
        };
        let chunk = chunk.map_err(|_| ApiProblem::bad_request("invalid_multipart"))?;
        let Some(chunk) = chunk else {
            return Ok(Forwarded::All);
        };
        if chunk.is_empty() {
            continue;
        }
        if idle_timeout(idle, sender.send(chunk)).await?.is_err() {
            return Ok(Forwarded::TransferEnded);
        }
    }
}
