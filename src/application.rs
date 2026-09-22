use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    api::{ApiServer, ApiServerConfig, DEFAULT_API_PORT},
    config::{ApiToken, FilesystemTrustStore, LocalIdentity, TrustStore, default_config_dir},
    protocol::DeviceType,
    transport::lan::{LanConfig, LanService, LocalDeviceInfo},
};

mod events;
mod service;
mod state;

pub use events::{ApplicationEvent, EventBus, EventBusError, EventData};
pub use service::{ApplicationError, ApplicationHandle, ApplicationService};
pub use state::{
    ClipboardSnapshot, Command, LocalDeviceSnapshot, OperationErrorCode, Pairing, PairingDirection,
    PairingSnapshot, PairingStatus, PairingTransitionError, Query, QueryResult, StatusSnapshot,
    Transfer, TransferDirection, TransferProgressError, TransferSnapshot, TransferStatus,
    TransferTransitionError,
};

/// Options for starting the MyConnect service.
#[derive(Debug, PartialEq, Eq)]
pub struct RunRequest {
    /// Directory in which received files should be stored.
    pub download_dir: Option<PathBuf>,
    /// Loopback port for the local authenticated control API.
    pub api_port: u16,
}

impl Default for RunRequest {
    fn default() -> Self {
        Self {
            download_dir: None,
            api_port: DEFAULT_API_PORT,
        }
    }
}

pub async fn run_service(request: RunRequest) -> Result<()> {
    let config_dir = default_config_dir().context("could not determine configuration directory")?;
    let identity = LocalIdentity::load_or_create(&config_dir)?;
    let token = ApiToken::load_or_create(&config_dir)?;
    let (application, commands) = ApplicationHandle::new(
        LocalDeviceSnapshot {
            device_id: identity.device_id().to_owned(),
            device_name: "MyConnect".to_owned(),
        },
        8,
        32,
        256,
    )?;
    let shutdown = CancellationToken::new();
    let trusted_device_ids = FilesystemTrustStore::new(&config_dir)
        .list()?
        .into_iter()
        .map(|device| device.device_id)
        .collect();
    let lan = LanService::start(
        LanConfig::default(),
        LocalDeviceInfo {
            device_id: identity.device_id().to_owned(),
            device_name: "MyConnect".to_owned(),
            device_type: DeviceType::Desktop,
            incoming_capabilities: Vec::new(),
            outgoing_capabilities: Vec::new(),
        },
        application.clone(),
        commands,
        trusted_device_ids,
        shutdown.clone(),
    )
    .await?;
    info!(tcp_address = %lan.tcp_addr(), "LAN transport listening");

    let server = ApiServer::start(
        ApiServerConfig::new(request.api_port)?,
        Arc::new(application),
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
    Ok(())
}
