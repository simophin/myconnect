//! Share: send a file to a paired device, and save files it sends here.
//!
//! Sending (`POST /devices/{id}/share`, a streamed upload) offers the file
//! with a `kdeconnect.share.request` that advertises a payload port, and
//! streams the upload's bytes to the device once it dials in. Receiving
//! dials the port a device's `kdeconnect.share.request` advertises and
//! saves the file in the download directory. Both run as transfers of the
//! core's transfers service, so they are listed, report progress and can be
//! cancelled through `/transfers` like any other.
//!
//! One file per request, in both directions. `kdeconnect.share.request.update`
//! (the size of a multi-file batch) isn't claimed, so the core drops it.
//!
//! Never log a file name or file contents.

mod http;
pub mod packet;

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::Router;
use bytes::Bytes;
use tokio::sync::mpsc;

pub use packet::{
    PACKET_TYPE, ShareRequestBody, ShareRequestUpdateBody, UPDATE_PACKET_TYPE,
    build_request_packet, build_update_packet, payload_port,
};

use crate::{
    core::{
        CoreError, DeviceSnapshot, OperationErrorCode, PayloadPeer, Plugin, PluginContext,
        TransferDirection, TransferHandle, TransferSnapshot, sanitize_file_name, upload_channel,
    },
    protocol::Packet,
};

pub struct SharePlugin;

impl Plugin for SharePlugin {
    fn id(&self) -> &'static str {
        "share"
    }

    fn incoming(&self) -> &'static [&'static str] {
        &[PACKET_TYPE]
    }

    fn outgoing(&self) -> &'static [&'static str] {
        &[PACKET_TYPE]
    }

    fn handle_packet(&self, ctx: &PluginContext, device: &DeviceSnapshot, packet: &Packet) {
        receive(ctx, device, packet);
    }

    fn streaming_routes(self: Arc<Self>, ctx: PluginContext) -> Router {
        http::streaming_routes(ctx)
    }
}

/// Start sending a file of `declared_size` bytes to a paired, connected
/// device that accepts `kdeconnect.share.request`. Returns at once with the
/// `queued` transfer and a bounded sender to stream the file's bytes into;
/// dropping the sender ends the upload. The transfer fails if fewer or more
/// bytes than declared arrive.
pub fn send_file(
    ctx: &PluginContext,
    device_id: &str,
    file_name: String,
    declared_size: u64,
) -> Result<(TransferSnapshot, mpsc::Sender<Bytes>), CoreError> {
    if file_name.trim().is_empty() {
        return Err(CoreError::InvalidFileName);
    }
    let limit = ctx.transfers().max_bytes();
    if declared_size > limit {
        return Err(CoreError::TransferTooLarge { limit });
    }
    ctx.can_send(device_id, PACKET_TYPE)?;
    let peer = ctx.payload_peer(device_id)?;
    let device = ctx.device(device_id).ok_or(CoreError::UnknownDevice)?;

    let transfer = ctx.transfers().begin(
        &device,
        TransferDirection::Outgoing,
        file_name,
        declared_size,
    );
    let started = transfer.snapshot();
    let (sender, chunks) = upload_channel();
    let ctx = ctx.clone();
    let device_id = device_id.to_owned();
    transfer.spawn(move |transfer| send(ctx, device_id, peer, transfer, chunks));
    Ok((started, sender))
}

/// Offer the file on a fresh payload port, wait for the device to dial in,
/// then stream the upload to it.
async fn send(
    ctx: PluginContext,
    device_id: String,
    peer: PayloadPeer,
    mut transfer: TransferHandle,
    mut chunks: mpsc::Receiver<Bytes>,
) {
    transfer.connecting();
    let Ok(listener) = peer.listen().await else {
        return transfer.fail(OperationErrorCode::ConnectionFailed);
    };
    let snapshot = transfer.snapshot();
    let Ok(packet) = build_request_packet(
        unix_millis(),
        snapshot.file_name,
        None,
        snapshot.total_bytes,
        listener.port(),
    ) else {
        return transfer.fail(OperationErrorCode::Internal);
    };
    if ctx.send(&device_id, packet).is_err() {
        return transfer.fail(OperationErrorCode::ConnectionFailed);
    }

    let cancellation = transfer.cancellation();
    let mut stream = tokio::select! {
        _ = cancellation.cancelled() => return transfer.cancelled(),
        accepted = listener.accept() => match accepted {
            Ok(stream) => stream,
            Err(_) => return transfer.fail(OperationErrorCode::ConnectionFailed),
        },
    };
    transfer.transferring();
    let result = transfer.forward(&mut chunks, &mut stream).await;
    transfer.finish(result);
}

/// Handle a `kdeconnect.share.request` from a paired device: check the
/// file's name and size, then dial the advertised payload port and save
/// the file. A request whose name can't be made safe, or that is over the
/// size limit, is recorded as a failed transfer without dialing.
fn receive(ctx: &PluginContext, device: &DeviceSnapshot, packet: &Packet) {
    let device_id = device.device_id.as_str();
    let Ok(body) = packet.body_as::<ShareRequestBody>() else {
        tracing::debug!(device_id, "dropping malformed share request");
        return;
    };
    let Some(total) = packet
        .payload_size
        .and_then(|size| u64::try_from(size).ok())
    else {
        tracing::debug!(device_id, "dropping share request without a payload size");
        return;
    };
    let Some(port) = packet.payload_transfer_info.as_ref().and_then(payload_port) else {
        tracing::debug!(device_id, "dropping share request without a payload port");
        return;
    };
    let Ok(peer) = ctx.payload_peer(device_id) else {
        return;
    };

    let transfers = ctx.transfers();
    let file_name = sanitize_file_name(&body.filename);
    let display_name = file_name.clone().unwrap_or(body.filename);
    let transfer = transfers.begin(device, TransferDirection::Incoming, display_name, total);
    let file_name = match file_name {
        Ok(_) if total > transfers.max_bytes() => {
            return transfer.fail(OperationErrorCode::Unavailable);
        }
        Ok(file_name) => file_name,
        Err(_) => return transfer.fail(OperationErrorCode::ProtocolError),
    };
    transfer.spawn(move |transfer| async move {
        transfer.connecting();
        let cancellation = transfer.cancellation();
        let mut stream = tokio::select! {
            _ = cancellation.cancelled() => return transfer.cancelled(),
            connected = peer.connect(port) => match connected {
                Ok(stream) => stream,
                Err(_) => return transfer.fail(OperationErrorCode::ConnectionFailed),
            },
        };
        transfer.save_to_downloads(&mut stream, &file_name).await;
    });
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::core::{
        Core, TransferStatus,
        testing::{handle_with_plugin, make_identity},
    };

    const PEER: &str = "740bd4b9b4184ee497d6caf1da8151be";

    /// A paired, connected peer that accepts share requests; its packets
    /// arrive on the returned receiver.
    fn paired_peer(handle: &Core) -> mpsc::Receiver<Packet> {
        let identity = make_identity(PEER, vec![PACKET_TYPE.into()]);
        handle.discover_device(&identity, true, 1).unwrap();
        let (tx, rx) = mpsc::channel(4);
        handle
            .register_connection(PEER, vec![1, 2, 3], 8, tx, CancellationToken::new(), 1)
            .unwrap();
        rx
    }

    #[test]
    fn sending_is_refused_before_a_transfer_is_recorded() {
        let (handle, _plugin, _commands) = handle_with_plugin(SharePlugin);
        let ctx = handle.plugin_context();
        assert!(matches!(
            send_file(&ctx, PEER, "a.txt".into(), 1),
            Err(CoreError::UnknownDevice)
        ));
        let _packets = paired_peer(&handle);
        assert!(matches!(
            send_file(&ctx, PEER, " ".into(), 1),
            Err(CoreError::InvalidFileName)
        ));
        assert!(matches!(
            send_file(&ctx, PEER, "a.txt".into(), u64::MAX),
            Err(CoreError::TransferTooLarge { .. })
        ));
        assert!(ctx.transfers().list().is_empty());
    }

    #[tokio::test]
    async fn sending_offers_the_file_on_a_payload_port() {
        let (handle, _plugin, _commands) = handle_with_plugin(SharePlugin);
        let mut packets = paired_peer(&handle);
        let ctx = handle.plugin_context();

        let (started, _sender) = send_file(&ctx, PEER, "notes.txt".into(), 5).unwrap();
        assert_eq!(started.status, TransferStatus::Queued);
        assert_eq!(started.direction, TransferDirection::Outgoing);

        let offer = tokio::time::timeout(std::time::Duration::from_secs(2), packets.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(offer.packet_type, PACKET_TYPE);
        assert_eq!(offer.payload_size, Some(5));
        assert!(
            offer
                .payload_transfer_info
                .as_ref()
                .and_then(payload_port)
                .is_some()
        );
        assert_eq!(
            offer.body_as::<ShareRequestBody>().unwrap().filename,
            "notes.txt"
        );

        handle.cancel_transfer(started.id).unwrap();
        handle
            .shutdown_transfers(std::time::Duration::from_secs(1))
            .await;
        assert_eq!(
            ctx.transfers().get(started.id).unwrap().status,
            TransferStatus::Cancelled
        );
    }

    #[test]
    fn requests_that_cannot_be_saved_safely_are_recorded_as_failed() {
        let (handle, _plugin, _commands) = handle_with_plugin(SharePlugin);
        let _packets = paired_peer(&handle);

        let traversal = build_request_packet(1_u64, "..".into(), None, 10, 1741).unwrap();
        handle.handle_peer_packet(PEER, traversal);
        let too_large =
            build_request_packet(2_u64, "big.bin".into(), None, u64::MAX, 1741).unwrap();
        handle.handle_peer_packet(PEER, too_large);

        let transfers = handle.transfers().list();
        let mut codes: Vec<_> = transfers
            .iter()
            .map(|transfer| {
                assert_eq!(transfer.status, TransferStatus::Failed);
                assert_eq!(transfer.direction, TransferDirection::Incoming);
                transfer.error_code
            })
            .collect();
        codes.sort_by_key(|code| format!("{code:?}"));
        assert_eq!(
            codes,
            [
                Some(OperationErrorCode::ProtocolError),
                Some(OperationErrorCode::Unavailable)
            ]
        );
    }
}
