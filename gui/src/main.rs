//! The MyConnect desktop app: the composition root for the daemon and its
//! UI in one process. It reads the flags and runs the UI
//! (`myconnect::ui`) with a way to start the daemon with every plugin; the
//! UI starts it (again on Retry) and shuts it down on exit.

// No console window behind the app on Windows, except in debug builds,
// where it shows the logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    time::Duration,
};

use anyhow::{Context, Result};
use clap::{Parser, builder::BoolishValueParser};
use myconnect::{
    client::API_TOKEN_ENV,
    config::ApiToken,
    daemon::{RunRequest, RunningService},
    plugins,
    ui::{self, UiOptions},
};

/// The MyConnect desktop app. Each flag can also be set through the
/// environment variable named after it.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Directory holding identity, trust and settings.
    #[arg(long, env = "MYCONNECT_DATA_DIR", value_name = "DIRECTORY")]
    data_dir: Option<PathBuf>,
    /// Directory received files are saved to, for this run only.
    #[arg(long, env = "MYCONNECT_DOWNLOAD_DIR", value_name = "DIRECTORY")]
    download_dir: Option<PathBuf>,
    /// Name this device advertises, for this run only.
    #[arg(long, env = "MYCONNECT_DEVICE_NAME", value_name = "NAME")]
    device_name: Option<String>,
    /// Keep discovery and connections on loopback, off the LAN.
    #[arg(long, env = "MYCONNECT_DISCOVERY_LOOPBACK", value_parser = BoolishValueParser::new())]
    discovery_loopback: bool,
    /// Sync an in-memory clipboard instead of the desktop's.
    #[arg(long, env = "MYCONNECT_NO_SYSTEM_CLIPBOARD", value_parser = BoolishValueParser::new())]
    no_system_clipboard: bool,
    /// Port of the HTTP API the CLI uses; 0 picks a free one.
    #[arg(
        long,
        env = "MYCONNECT_API_PORT",
        value_name = "PORT",
        default_value_t = 0
    )]
    api_port: u16,
    /// Token the CLI must present; a random one by default.
    #[arg(long, env = API_TOKEN_ENV, value_name = "TOKEN", hide_env_values = true)]
    api_token: Option<String>,
    /// Add made-up paired devices that change over time, to see the UI
    /// without real ones.
    #[arg(long)]
    demo: bool,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wgpu=warn,naga=warn".into()),
        )
        .init();
    let args = Args::parse();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("myconnect")
        .build()
        .context("could not start async runtime")?;
    let api_token = match args.api_token {
        Some(secret) => ApiToken::from_secret(secret)?,
        None => ApiToken::generate(),
    };
    let data_dir = args.data_dir.clone();
    let request = RunRequest {
        api_token: Some(api_token),
        data_dir: args.data_dir,
        download_dir: args.download_dir,
        device_name: args.device_name,
        discovery_loopback: args.discovery_loopback,
        system_clipboard: !args.no_system_clipboard,
        api_host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        api_port: args.api_port,
    };
    let start = move || -> ui::StartFuture {
        let request = request.clone();
        Box::pin(async move {
            let mut ui_plugins = None;
            let service = RunningService::start_with(request, |clipboard| {
                let parts = plugins::builtin_parts(clipboard);
                ui_plugins = Some((parts.clipboard, parts.browse));
                parts.core
            })
            .await?;
            let (clipboard, browse) = ui_plugins.expect("the daemon built its plugins");
            tracing::info!(
                device_id = service.core().local_device_id(),
                api = %service.api_addr(),
                "daemon started"
            );
            Ok(ui::Started {
                service,
                clipboard,
                browse,
            })
        })
    };

    let result = ui::run(
        UiOptions {
            runtime: runtime.handle().clone(),
            demo: args.demo,
            version: env!("MYCONNECT_APP_VERSION").into(),
            data_dir,
        },
        start,
    );
    runtime.shutdown_timeout(Duration::from_secs(2));
    result
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn arguments_are_well_formed() {
        Args::command().debug_assert();
    }
}
