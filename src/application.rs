use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::{
    api::{ApiServer, ApiServerConfig, DEFAULT_API_PORT},
    config::{ApiToken, LocalIdentity, default_config_dir},
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

/// A request from any MyConnect frontend.
#[derive(Debug, PartialEq, Eq)]
pub enum Request {
    /// Start the long-running MyConnect service.
    Run(RunRequest),
    /// Send a file to a device.
    Send(SendRequest),
}

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

/// Options for sending a file to a device.
#[derive(Debug, PartialEq, Eq)]
pub struct SendRequest {
    /// Device name or identifier selected by the user.
    pub device: String,
    /// File to send.
    pub file: PathBuf,
}

/// Execute a frontend request.
///
/// This is the seam shared by the CLI and future GUI binaries. Protocol,
/// discovery, and transfer implementations will be connected here as they are
/// introduced.
pub async fn execute(request: Request) -> Result<()> {
    match request {
        Request::Run(request) => {
            run_service(request).await?;
        }
        Request::Send(request) => {
            warn!(device = %request.device, file = %request.file.display(), "file transfer is not implemented yet");
        }
    }

    Ok(())
}

async fn run_service(request: RunRequest) -> Result<()> {
    let config_dir = default_config_dir().context("could not determine configuration directory")?;
    let identity = LocalIdentity::load_or_create(&config_dir)?;
    let token = ApiToken::load_or_create(&config_dir)?;
    let (application, mut commands) = ApplicationHandle::new(
        LocalDeviceSnapshot {
            device_id: identity.device_id().to_owned(),
            device_name: "MyConnect".to_owned(),
        },
        8,
        32,
        256,
    )?;
    let shutdown = CancellationToken::new();
    let command_shutdown = shutdown.clone();
    let command_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = command_shutdown.cancelled() => break,
                command = commands.recv() => match command {
                    Some(Command::AnnounceDiscovery) => {
                        // LAN announcement is connected in Phase 6.
                        info!("discovery announcement requested");
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        }
    });

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
    timeout(Duration::from_secs(5), command_task)
        .await
        .context("application command task did not stop before its deadline")??;
    Ok(())
}
