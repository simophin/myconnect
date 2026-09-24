use std::{
    net::{IpAddr, Ipv6Addr},
    path::PathBuf,
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use myconnect::{
    api::DEFAULT_API_PORT,
    application::{
        ApplicationEvent, ClipboardSnapshot, EventData, PairingSnapshot, RunRequest,
        TransferSnapshot,
    },
    client::{ApiClient, ClipboardWatchUpdate, DeviceWatchUpdate, TransferWatchUpdate},
    device::DeviceSnapshot,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Connect and communicate with your devices.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    /// Emit newline-delimited JSON rather than human-readable output.
    #[arg(long, global = true)]
    json: bool,
    /// Host of the local control API: listened on by `run`, connected to by
    /// every other command. Defaults to 127.0.0.1.
    #[arg(long, global = true, value_name = "HOST")]
    api_host: Option<String>,
    /// Port of the local control API: listened on by `run`, connected to by
    /// every other command. Defaults to 24816.
    #[arg(long, global = true, value_name = "PORT")]
    api_port: Option<u16>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, PartialEq, Eq, Subcommand)]
enum Command {
    /// Run the MyConnect daemon in the foreground.
    Run {
        #[arg(long, value_name = "DIRECTORY")]
        download_dir: Option<PathBuf>,
        /// Directory holding local identity, trust, and token state.
        #[arg(long, value_name = "DIRECTORY")]
        data_dir: Option<PathBuf>,
        /// Name this device advertises to peers.
        #[arg(long, value_name = "NAME")]
        device_name: Option<String>,
        /// Restrict LAN discovery to loopback instead of the real network,
        /// so multiple local instances can discover each other without a
        /// second machine. Real devices on the LAN will not be discovered.
        #[arg(long)]
        discovery_loopback: bool,
    },
    /// List known devices.
    Devices {
        #[arg(long)]
        watch: bool,
    },
    /// Broadcast a discovery request and list unpaired devices that answer.
    Scan {
        /// Seconds to wait for devices to respond before listing results.
        #[arg(long, default_value_t = 3)]
        timeout: u64,
        /// Keep listening and print unpaired devices as they appear.
        #[arg(long)]
        watch: bool,
    },
    /// Start, accept, or reject pairing.
    Pair {
        #[arg(value_name = "DEVICE_ID | ACTION PAIRING_ID", num_args = 1..=2, required = true)]
        arguments: Vec<String>,
    },
    /// Unpair and forget a device.
    Unpair { device_id: String },
    /// Ping a paired device, optionally with a message.
    Ping {
        device_id: String,
        message: Option<String>,
    },
    /// Send a file to a paired device.
    Send {
        device_id: String,
        file: PathBuf,
        #[arg(long)]
        watch: bool,
    },
    /// Read, update, or watch synchronized clipboard text.
    Clipboard {
        #[command(subcommand)]
        action: ClipboardAction,
    },
}

enum PairAction {
    Start(String),
    Accept(Uuid),
    Reject(Uuid),
}

#[derive(Debug, PartialEq, Eq, Subcommand)]
enum ClipboardAction {
    Get,
    Set { text: String },
    Watch,
}

impl Cli {
    pub async fn execute(self) -> Result<()> {
        let Self {
            json,
            api_host,
            api_port,
            command,
        } = self;
        if let Command::Run {
            download_dir,
            data_dir,
            device_name,
            discovery_loopback,
        } = command
        {
            let mut request = RunRequest {
                download_dir,
                data_dir,
                device_name,
                discovery_loopback,
                ..RunRequest::default()
            };
            if let Some(host) = api_host {
                request.api_host = host
                    .parse::<IpAddr>()
                    .with_context(|| format!("invalid --api-host address: {host}"))?;
            }
            if let Some(port) = api_port {
                request.api_port = port;
            }
            return myconnect::application::run_service(request).await;
        }

        let base_url_override = (api_host.is_some() || api_port.is_some()).then(|| {
            let host = api_host.unwrap_or_else(|| "127.0.0.1".to_owned());
            let port = api_port.unwrap_or(DEFAULT_API_PORT);
            format!("http://{}:{port}", format_host_for_url(&host))
        });
        let client = ApiClient::from_environment_with_url(base_url_override)?;
        match command {
            Command::Run { .. } => unreachable!("run handled before client configuration"),
            Command::Devices { watch: false } => print_devices(&client.devices().await?, json),
            Command::Devices { watch: true } => {
                client
                    .watch_devices(cancellation_on_ctrl_c(), |update| match update {
                        DeviceWatchUpdate::Snapshot(devices) => print_devices(&devices, json),
                        DeviceWatchUpdate::Event(event) => print_event(&event, json),
                    })
                    .await?;
            }
            Command::Scan { timeout, watch } => {
                client.scan().await?;
                if watch {
                    client
                        .watch_devices(cancellation_on_ctrl_c(), |update| match update {
                            DeviceWatchUpdate::Snapshot(devices) => {
                                print_devices(&unpaired(devices), json)
                            }
                            DeviceWatchUpdate::Event(event) if event_device_unpaired(&event) => {
                                print_event(&event, json)
                            }
                            DeviceWatchUpdate::Event(_) => {}
                        })
                        .await?;
                } else {
                    tokio::time::sleep(std::time::Duration::from_secs(timeout)).await;
                    print_devices(&unpaired(client.devices().await?), json);
                }
            }
            Command::Pair { arguments } => match parse_pair_action(&arguments)? {
                PairAction::Start(device_id) => {
                    print_pairing(&client.start_pairing(&device_id).await?, json)
                }
                PairAction::Accept(pairing_id) => {
                    print_pairing(&client.accept_pairing(pairing_id).await?, json)
                }
                PairAction::Reject(pairing_id) => {
                    client.reject_pairing(pairing_id).await?;
                    if json {
                        println!("{}", json!({"pairingId": pairing_id, "status": "rejected"}));
                    } else {
                        println!("Pairing {pairing_id} rejected");
                    }
                }
            },
            Command::Unpair { device_id } => {
                client.unpair(&device_id).await?;
                if json {
                    println!("{}", json!({"deviceId": device_id, "status": "unpaired"}));
                } else {
                    println!("Device {device_id} unpaired");
                }
            }
            Command::Ping { device_id, message } => {
                client.ping(&device_id, message.as_deref()).await?;
                if json {
                    println!("{}", json!({"deviceId": device_id, "status": "sent"}));
                } else {
                    println!("Ping sent to {device_id}");
                }
            }
            Command::Send {
                device_id,
                file,
                watch,
            } => {
                let transfer = client.send_file(&device_id, &file).await?;
                if watch {
                    client
                        .watch_transfer(
                            transfer.id,
                            cancellation_on_ctrl_c(),
                            |update| match update {
                                TransferWatchUpdate::Snapshot(transfer) => {
                                    print_transfer(&transfer, json)
                                }
                                TransferWatchUpdate::Event(event) => print_event(&event, json),
                            },
                        )
                        .await?;
                } else {
                    print_transfer(&transfer, json);
                }
            }
            Command::Clipboard {
                action: ClipboardAction::Get,
            } => print_clipboard(&client.clipboard().await?, json),
            Command::Clipboard {
                action: ClipboardAction::Set { text },
            } => print_clipboard(&client.set_clipboard(&text).await?, json),
            Command::Clipboard {
                action: ClipboardAction::Watch,
            } => {
                client
                    .watch_clipboard(cancellation_on_ctrl_c(), |update| match update {
                        ClipboardWatchUpdate::Snapshot(clipboard) => {
                            print_clipboard(&clipboard, json)
                        }
                        ClipboardWatchUpdate::Event(event) => print_event(&event, json),
                    })
                    .await?;
            }
        }
        Ok(())
    }
}

fn parse_pair_action(arguments: &[String]) -> Result<PairAction> {
    match arguments {
        [device_id] if !matches!(device_id.as_str(), "accept" | "reject") => {
            Ok(PairAction::Start(device_id.clone()))
        }
        [action, pairing_id] if matches!(action.as_str(), "accept" | "reject") => {
            let pairing_id = pairing_id.parse::<Uuid>()?;
            if action == "accept" {
                Ok(PairAction::Accept(pairing_id))
            } else {
                Ok(PairAction::Reject(pairing_id))
            }
        }
        _ => anyhow::bail!(
            "usage: myconnect pair <device-id> | myconnect pair accept|reject <pairing-id>"
        ),
    }
}

/// Bracket a bare IPv6 address so it forms a valid URL host, leaving IPv4
/// addresses and hostnames unchanged.
fn format_host_for_url(host: &str) -> String {
    match host.parse::<Ipv6Addr>() {
        Ok(address) => format!("[{address}]"),
        Err(_) => host.to_owned(),
    }
}

fn cancellation_on_ctrl_c() -> CancellationToken {
    let cancellation = CancellationToken::new();
    let signal = cancellation.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal.cancel();
        }
    });
    cancellation
}

fn unpaired(devices: Vec<DeviceSnapshot>) -> Vec<DeviceSnapshot> {
    devices
        .into_iter()
        .filter(|device| !device.paired)
        .collect()
}

fn event_device_unpaired(event: &ApplicationEvent) -> bool {
    match &event.event {
        EventData::DeviceDiscovered(device)
        | EventData::DeviceConnected(device)
        | EventData::DeviceUpdated(device)
        | EventData::DeviceDisconnected(device) => !device.paired,
        _ => false,
    }
}

fn print_devices(devices: &[DeviceSnapshot], json_output: bool) {
    if json_output {
        println!(
            "{}",
            serde_json::to_string(devices).expect("snapshot serializes")
        );
    } else if devices.is_empty() {
        println!("No devices found");
    } else {
        for device in devices {
            let trust = if device.paired { "paired" } else { "unpaired" };
            println!(
                "{}\t{}\t{}\t{}",
                device.device_id,
                device.device_name,
                enum_name(device.reachability),
                trust
            );
        }
    }
}

fn print_pairing(pairing: &PairingSnapshot, json_output: bool) {
    if json_output {
        println!(
            "{}",
            serde_json::to_string(pairing).expect("snapshot serializes")
        );
    } else {
        println!(
            "Pairing {} with {}: {}",
            pairing.id,
            pairing.device_name,
            enum_name(pairing.status)
        );
        if let Some(code) = &pairing.verification_code {
            println!("Verification code: {code}");
        }
    }
}

fn print_transfer(transfer: &TransferSnapshot, json_output: bool) {
    if json_output {
        println!(
            "{}",
            serde_json::to_string(transfer).expect("snapshot serializes")
        );
    } else {
        println!(
            "Transfer {}: {} ({}/{})",
            transfer.id,
            enum_name(transfer.status),
            transfer.transferred_bytes,
            transfer.total_bytes
        );
    }
}

fn print_clipboard(clipboard: &ClipboardSnapshot, json_output: bool) {
    if json_output {
        println!(
            "{}",
            serde_json::to_string(clipboard).expect("snapshot serializes")
        );
    } else {
        println!("{}", clipboard.text);
    }
}

fn print_event(event: &ApplicationEvent, json_output: bool) {
    if json_output {
        println!(
            "{}",
            serde_json::to_string(event).expect("event serializes")
        );
    } else {
        match &event.event {
            EventData::DeviceDiscovered(device)
            | EventData::DeviceConnected(device)
            | EventData::DeviceUpdated(device)
            | EventData::DeviceDisconnected(device) => {
                println!(
                    "Device {}: {}",
                    device.device_name,
                    enum_name(device.reachability)
                )
            }
            EventData::ClipboardChanged(clipboard) => println!("{}", clipboard.text),
            EventData::TransferStarted(transfer)
            | EventData::TransferProgress(transfer)
            | EventData::TransferCompleted(transfer)
            | EventData::TransferFailed(transfer) => print_transfer(transfer, false),
            EventData::PairingRequested(pairing) | EventData::PairingUpdated(pairing) => {
                print_pairing(pairing, false)
            }
        }
    }
}

fn enum_name(value: impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .expect("enum serializes")
        .as_str()
        .expect("enum is a string")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "00000000-0000-0000-0000-000000000001";

    #[test]
    fn parses_every_command() {
        let cases = [
            vec!["myconnect", "run", "--api-port", "25000"],
            vec!["myconnect", "run", "--data-dir", "/tmp/myconnect"],
            vec!["myconnect", "run", "--device-name", "My Desktop"],
            vec!["myconnect", "run", "--discovery-loopback"],
            vec![
                "myconnect",
                "--api-host",
                "0.0.0.0",
                "--api-port",
                "25000",
                "run",
            ],
            vec!["myconnect", "--api-host", "192.168.1.5", "devices"],
            vec!["myconnect", "devices"],
            vec!["myconnect", "devices", "--watch"],
            vec!["myconnect", "scan"],
            vec!["myconnect", "scan", "--timeout", "5"],
            vec!["myconnect", "scan", "--watch"],
            vec!["myconnect", "ping", "device-id"],
            vec!["myconnect", "ping", "device-id", "hello"],
            vec!["myconnect", "pair", "device-id"],
            vec!["myconnect", "pair", "accept", ID],
            vec!["myconnect", "pair", "reject", ID],
            vec!["myconnect", "unpair", "device-id"],
            vec!["myconnect", "send", "device-id", "photo.jpg"],
            vec!["myconnect", "send", "device-id", "photo.jpg", "--watch"],
            vec!["myconnect", "clipboard", "get"],
            vec!["myconnect", "clipboard", "set", "hello"],
            vec!["myconnect", "clipboard", "watch"],
            vec!["myconnect", "--json", "devices"],
        ];
        for arguments in cases {
            Cli::try_parse_from(&arguments)
                .unwrap_or_else(|error| panic!("failed to parse {arguments:?}: {error}"));
        }
    }

    #[test]
    fn pairing_requires_exactly_one_action() {
        assert!(Cli::try_parse_from(["myconnect", "pair"]).is_err());
        assert!(Cli::try_parse_from(["myconnect", "pair", "device", "accept", ID]).is_err());
        assert!(parse_pair_action(&["accept".into()]).is_err());
    }
}
