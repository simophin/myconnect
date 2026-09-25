use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::{
    api::{ApiServer, ApiServerConfig, DEFAULT_API_PORT},
    clipboard::{ClipboardService, InMemoryClipboard, SystemClipboard},
    config::{
        ApiToken, FilesystemTrustStore, LocalIdentity, SettingsFile, StoredSettings, TrustStore,
        default_config_dir,
    },
    plugins,
    protocol::{DeviceType, is_forbidden_name_character, is_valid_device_name},
    transport::{
        lan::{DISCOVERY_PORT, LanConfig, LanService, LocalDeviceInfo},
        tls::subject_public_key_info,
    },
};

mod events;
mod files;
mod plugin;
mod service;
mod settings;
mod state;
#[cfg(test)]
pub(crate) mod testing;
mod transfer;

use settings::Settings;

pub use events::{ApplicationEvent, EventBus, EventBusError, EventData};
pub use files::{DirectoryListing, FileEntry, FileKind};
pub use plugin::{Plugin, PluginContext, PluginEvent, PluginEventKind, PluginRegistry};
pub use service::{ApplicationError, ApplicationHandle, ApplicationService, RemoteFileContent};
pub use settings::{SettingsDefaults, SettingsPatch, SettingsSnapshot};
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
    /// Bearer token API clients must present. `None` leaves the control API
    /// unauthenticated.
    pub api_token: Option<ApiToken>,
    /// Directory in which received files should be stored. Overrides the
    /// stored setting for this run only.
    pub download_dir: Option<PathBuf>,
    /// Directory holding local identity and trust state. Defaults to
    /// the platform configuration directory.
    pub data_dir: Option<PathBuf>,
    /// Name this device advertises to peers. Overrides the stored setting
    /// for this run only; with neither, the host name is used.
    pub device_name: Option<String>,
    /// Address the local control API listens on.
    pub api_host: IpAddr,
    /// Port the local control API listens on; `0` picks a free port, which
    /// [`RunningService::api_addr`] then reports.
    pub api_port: u16,
    /// Restrict LAN discovery broadcasts to loopback instead of the real
    /// network. A physical switch never reflects a broadcast frame back to
    /// the port it arrived on, so two instances on the same host normally
    /// can't discover each other over a real NIC; loopback broadcast does
    /// not have that limitation. Useful for running multiple local
    /// instances against each other without a second machine, at the cost
    /// of not discovering real devices on the LAN.
    pub discovery_loopback: bool,
    /// Sync the desktop clipboard rather than an in-memory one. Falls back to
    /// the in-memory clipboard, with a warning, when the session has no
    /// usable clipboard (e.g. no display server).
    pub system_clipboard: bool,
}

impl Default for RunRequest {
    fn default() -> Self {
        Self {
            api_token: None,
            download_dir: None,
            data_dir: None,
            device_name: None,
            api_host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            api_port: DEFAULT_API_PORT,
            discovery_loopback: false,
            system_clipboard: false,
        }
    }
}

/// A started daemon: LAN transport, application core, and control API.
///
/// The CLI runs one until Ctrl-C; an embedding frontend (see the `ffi` crate)
/// starts one, reads [`RunningService::api_addr`], and shuts it down on exit.
pub struct RunningService {
    application: ApplicationHandle,
    lan: LanService,
    server: ApiServer,
    system_clipboard: Option<(Arc<SystemClipboard>, JoinHandle<()>)>,
}

impl RunningService {
    pub async fn start(request: RunRequest) -> Result<Self> {
        let config_dir = request
            .data_dir
            .clone()
            .or_else(default_config_dir)
            .context("could not determine configuration directory")?;
        let identity = Arc::new(LocalIdentity::load_or_create(&config_dir)?);
        let trust_store: Arc<dyn TrustStore + Send + Sync> =
            Arc::new(FilesystemTrustStore::new(&config_dir));
        let local_public_key_der = subject_public_key_info(identity.certificate_der())
            .context("local identity certificate could not be parsed")?;
        let settings_file = SettingsFile::new(&config_dir);
        let stored = settings_file.load().unwrap_or_else(|error| {
            // Start with defaults rather than not at all; the file is
            // rewritten on the next change.
            warn!(%error, "ignoring unreadable settings file");
            StoredSettings::default()
        });
        let settings = Settings::new(SettingsDefaults {
            device_name: default_device_name(),
            download_dir: default_download_dir().unwrap_or_else(|| config_dir.join("downloads")),
        })
        .with_file(settings_file, stored)
        .with_overrides(StoredSettings {
            device_name: request.device_name.clone(),
            download_dir: request
                .download_dir
                .as_deref()
                .map(|directory| std::path::absolute(directory).unwrap_or(directory.into())),
            ..StoredSettings::default()
        });
        let initial = settings.snapshot();
        let device_name = initial.device_name.clone();
        let system_clipboard = if request.system_clipboard {
            match SystemClipboard::start() {
                Ok(clipboard) => Some(Arc::new(clipboard)),
                Err(error) => {
                    warn!(%error, "using an in-memory clipboard instead");
                    None
                }
            }
        } else {
            None
        };
        let clipboard: Arc<dyn ClipboardService + Send + Sync> = match &system_clipboard {
            Some(clipboard) => clipboard.clone(),
            None => InMemoryClipboard::shared(),
        };
        let (application, commands) = ApplicationHandle::new(
            LocalDeviceSnapshot {
                device_id: identity.device_id().to_owned(),
                device_name: device_name.clone(),
            },
            8,
            local_public_key_der,
            trust_store.clone(),
            clipboard,
            32,
            256,
            identity.clone(),
            TransferConfig::new(initial.download_dir),
        )?;
        application.install_settings(settings);
        let shutdown = CancellationToken::new();
        let system_clipboard = system_clipboard.map(|clipboard| {
            let follower = application
                .follow_local_clipboard(clipboard.local_changes(), shutdown.child_token());
            (clipboard, follower)
        });
        let capabilities = plugins::capabilities();
        let mut lan_config = LanConfig::default();
        if request.discovery_loopback {
            // The bind stays on the wildcard address: a socket bound to a single
            // address only accepts packets addressed to that exact address, so
            // binding to 127.0.0.1 specifically would silently drop incoming
            // packets addressed to the 127.255.255.255 broadcast below. Only the
            // announce target needs to change to keep discovery off the real
            // network.
            lan_config = lan_config.with_announcement_targets(vec![SocketAddr::V4(
                SocketAddrV4::new(Ipv4Addr::new(127, 255, 255, 255), DISCOVERY_PORT),
            )]);
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
            request.api_token,
            shutdown.clone(),
        )
        .await?;
        info!(address = %server.local_addr(), "local control API listening");

        Ok(Self {
            application,
            lan,
            server,
            system_clipboard,
        })
    }

    /// The address the control API actually bound, including the port chosen
    /// by the OS when the request asked for port `0`.
    pub fn api_addr(&self) -> SocketAddr {
        self.server.local_addr()
    }

    /// Stop the control API and LAN transport, then give in-flight transfers
    /// a bounded window to clean up their partial files.
    pub async fn shutdown(self) -> Result<()> {
        let Self {
            application,
            lan,
            server,
            system_clipboard,
        } = self;
        let server_result = server.shutdown().await;
        let lan_result = lan.shutdown().await;
        if let Some((clipboard, follower)) = system_clipboard {
            follower.abort();
            // Joining the clipboard thread may wait briefly for a clipboard
            // manager to take over the text we own (X11).
            let _ = tokio::task::spawn_blocking(move || clipboard.stop()).await;
        }
        application.shutdown_transfers(Duration::from_secs(5)).await;
        application.shutdown_browsing().await;
        server_result?;
        lan_result?;
        Ok(())
    }
}

/// Run the daemon in the foreground until Ctrl-C.
pub async fn run_service(request: RunRequest) -> Result<()> {
    let service = RunningService::start(request).await?;
    tokio::signal::ctrl_c()
        .await
        .context("failed to listen for shutdown signal")?;
    service.shutdown().await
}

/// The host name, trimmed to its first label and to what a KDE Connect
/// device name allows, or "MyConnect" if nothing usable is left.
fn default_device_name() -> String {
    device_name_from_host(&gethostname::gethostname().to_string_lossy())
}

fn device_name_from_host(host: &str) -> String {
    let label = host.split('.').next().unwrap_or_default();
    let name: String = label
        .chars()
        .filter(|character| !character.is_control() && !is_forbidden_name_character(*character))
        .take(32)
        .collect();
    let name = name.trim();
    if is_valid_device_name(name) {
        name.to_owned()
    } else {
        "MyConnect".to_owned()
    }
}

/// The platform download directory, if one can be determined. Falls back to
/// a `downloads` directory under the MyConnect configuration directory.
fn default_download_dir() -> Option<PathBuf> {
    directories::UserDirs::new().and_then(|dirs| dirs.download_dir().map(PathBuf::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_names_from_host_names_fit_the_identity_schema() {
        assert_eq!(device_name_from_host("desk"), "desk");
        assert_eq!(device_name_from_host("desk.example.org"), "desk");
        assert_eq!(
            device_name_from_host("a-very-long-host-name-that-keeps-on-going"),
            "a-very-long-host-name-that-keeps"
        );
        assert_eq!(device_name_from_host("(desk)"), "desk");
        assert_eq!(device_name_from_host(""), "MyConnect");
        assert_eq!(device_name_from_host(".local"), "MyConnect");
    }
}
