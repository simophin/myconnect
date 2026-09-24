use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    api::{ApiServer, ApiServerConfig, DEFAULT_API_PORT},
    clipboard::InMemoryClipboard,
    config::{ApiToken, FilesystemTrustStore, LocalIdentity, TrustStore, default_config_dir},
    plugins,
    protocol::DeviceType,
    transport::{
        lan::{DISCOVERY_PORT, LanConfig, LanService, LocalDeviceInfo},
        tls::subject_public_key_info,
    },
};

mod events;
mod service;
mod state;
mod transfer;

pub use events::{ApplicationEvent, EventBus, EventBusError, EventData};
pub use service::{ApplicationError, ApplicationHandle, ApplicationService};
pub use state::{
    ClipboardSnapshot, Command, LocalDeviceSnapshot, MAX_CLIPBOARD_TEXT_BYTES, OperationErrorCode,
    Pairing, PairingDirection, PairingSnapshot, PairingStatus, PairingTransitionError, Query,
    QueryResult, StatusSnapshot, Transfer, TransferDirection, TransferProgressError,
    TransferSnapshot, TransferStatus, TransferTransitionError,
};
pub use transfer::{DEFAULT_MAX_TRANSFER_BYTES, FileNameError, TransferConfig};

/// Options for starting the MyConnect service.
#[derive(Debug, PartialEq, Eq)]
pub struct RunRequest {
    /// Directory in which received files should be stored.
    pub download_dir: Option<PathBuf>,
    /// Directory holding local identity, trust, and token state. Defaults to
    /// the platform configuration directory.
    pub data_dir: Option<PathBuf>,
    /// Name this device advertises to peers. Defaults to "MyConnect".
    pub device_name: Option<String>,
    /// Address the local authenticated control API listens on.
    pub api_host: IpAddr,
    /// Port the local authenticated control API listens on.
    pub api_port: u16,
    /// Restrict LAN discovery broadcasts to loopback instead of the real
    /// network. A physical switch never reflects a broadcast frame back to
    /// the port it arrived on, so two instances on the same host normally
    /// can't discover each other over a real NIC; loopback broadcast does
    /// not have that limitation. Useful for running multiple local
    /// instances against each other without a second machine, at the cost
    /// of not discovering real devices on the LAN.
    pub discovery_loopback: bool,
}

impl Default for RunRequest {
    fn default() -> Self {
        Self {
            download_dir: None,
            data_dir: None,
            device_name: None,
            api_host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            api_port: DEFAULT_API_PORT,
            discovery_loopback: false,
        }
    }
}

pub async fn run_service(request: RunRequest) -> Result<()> {
    let config_dir = request
        .data_dir
        .clone()
        .or_else(default_config_dir)
        .context("could not determine configuration directory")?;
    let identity = Arc::new(LocalIdentity::load_or_create(&config_dir)?);
    let token = ApiToken::load_or_create(&config_dir)?;
    let trust_store: Arc<dyn TrustStore + Send + Sync> =
        Arc::new(FilesystemTrustStore::new(&config_dir));
    let local_public_key_der = subject_public_key_info(identity.certificate_der())
        .context("local identity certificate could not be parsed")?;
    let download_dir = request
        .download_dir
        .clone()
        .or_else(default_download_dir)
        .unwrap_or_else(|| config_dir.join("downloads"));
    let device_name = request
        .device_name
        .clone()
        .unwrap_or_else(|| "MyConnect".to_owned());
    let (application, commands) = ApplicationHandle::new(
        LocalDeviceSnapshot {
            device_id: identity.device_id().to_owned(),
            device_name: device_name.clone(),
        },
        8,
        local_public_key_der,
        trust_store.clone(),
        InMemoryClipboard::shared(),
        32,
        256,
        identity.clone(),
        TransferConfig::new(download_dir),
    )?;
    let shutdown = CancellationToken::new();
    let capabilities = plugins::capabilities();
    let mut lan_config = LanConfig::default();
    if request.discovery_loopback {
        // The bind stays on the wildcard address: a socket bound to a single
        // address only accepts packets addressed to that exact address, so
        // binding to 127.0.0.1 specifically would silently drop incoming
        // packets addressed to the 127.255.255.255 broadcast below. Only the
        // announce target needs to change to keep discovery off the real
        // network.
        lan_config = lan_config.with_announcement_targets(vec![SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(127, 255, 255, 255),
            DISCOVERY_PORT,
        ))]);
    }
    let lan = LanService::start(
        lan_config,
        LocalDeviceInfo {
            device_id: identity.device_id().to_owned(),
            device_name,
            device_type: DeviceType::Desktop,
            incoming_capabilities: capabilities.incoming,
            outgoing_capabilities: capabilities.outgoing,
        },
        application.clone(),
        commands,
        identity,
        trust_store,
        shutdown.clone(),
    )
    .await?;
    info!(tcp_address = %lan.tcp_addr(), "LAN transport listening");

    let server = ApiServer::start(
        ApiServerConfig::new(request.api_port)?.with_host(request.api_host),
        Arc::new(application.clone()),
        token,
        shutdown.clone(),
    )
    .await?;
    info!(address = %server.local_addr(), "local control API listening");

    tokio::signal::ctrl_c()
        .await
        .context("failed to listen for shutdown signal")?;
    server.shutdown().await?;
    lan.shutdown().await?;
    application.shutdown_transfers(Duration::from_secs(5)).await;
    Ok(())
}

/// The platform download directory, if one can be determined. Falls back to
/// a `downloads` directory under the MyConnect configuration directory.
fn default_download_dir() -> Option<PathBuf> {
    directories::UserDirs::new().and_then(|dirs| dirs.download_dir().map(PathBuf::from))
}
