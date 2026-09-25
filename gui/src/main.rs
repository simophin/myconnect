//! Spike: a native MyConnect desktop UI in iced.
//!
//! The UI runs the daemon in-process and reads the core directly: a
//! snapshot, then the core's event stream, and a fresh snapshot whenever
//! the stream lags. The daemon still serves its HTTP API, so the CLI can
//! drive the same instance.

mod demo;

use std::{
    hash::{Hash, Hasher},
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    time::Duration,
};

use anyhow::{Context, Result};
use clap::Parser;
use futures::{SinkExt, Stream};
use iced::{
    Alignment, Background, Border, Color, Element, Font, Length, Subscription, Theme, font,
    widget::{Space, column, container, row, scrollable, space, text},
};
use iced_fonts::lucide;
use myconnect::{
    config::ApiToken,
    core::{Core, CoreEvent, DeviceReachability, DeviceSnapshot, EventData},
    daemon::{RunRequest, RunningService},
    protocol::DeviceType,
};
use tokio::sync::broadcast::error::RecvError;

#[derive(Debug, Parser)]
#[command(about = "MyConnect desktop UI (iced spike)")]
struct Args {
    /// Directory holding identity, trust and settings.
    #[arg(long)]
    data_dir: Option<PathBuf>,
    /// Directory received files are saved to.
    #[arg(long)]
    download_dir: Option<PathBuf>,
    /// Name this device advertises.
    #[arg(long)]
    device_name: Option<String>,
    /// Keep discovery and connections on loopback, off the LAN.
    #[arg(long)]
    discovery_loopback: bool,
    /// Port for the HTTP API the CLI uses; 0 picks a free one.
    #[arg(long, default_value_t = 0)]
    api_port: u16,
    /// Token the CLI must present; a random one by default.
    #[arg(long, env = "MYCONNECT_API_TOKEN")]
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
    let service = runtime.block_on(RunningService::start(RunRequest {
        api_token: Some(api_token),
        data_dir: args.data_dir,
        download_dir: args.download_dir,
        device_name: args.device_name,
        discovery_loopback: args.discovery_loopback,
        system_clipboard: true,
        api_host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        api_port: args.api_port,
    }))?;
    tracing::info!(
        device_id = service.core().local_device_id(),
        api = %service.api_addr(),
        "daemon started"
    );

    if args.demo {
        runtime.spawn(demo::run(service.core().clone()));
    }

    let daemon = Daemon(service.core().clone());
    let result = iced::application(move || App::new(daemon.clone()), App::update, App::view)
        .title("MyConnect")
        .subscription(App::subscription)
        .font(iced_fonts::LUCIDE_FONT_BYTES)
        .window_size((440.0, 620.0))
        .run();

    runtime.block_on(service.shutdown())?;
    runtime.shutdown_timeout(Duration::from_secs(2));
    result.context("UI failed")
}

/// The in-process core, as the key of the subscription that watches it.
#[derive(Clone)]
struct Daemon(Core);

impl Hash for Daemon {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.local_device_id().hash(state);
    }
}

#[derive(Debug, Clone)]
enum Message {
    Snapshot {
        devices: Vec<DeviceSnapshot>,
        local_name: String,
    },
    Event(Box<CoreEvent>),
}

struct App {
    daemon: Daemon,
    devices: DeviceList,
}

impl App {
    fn new(daemon: Daemon) -> Self {
        let devices = DeviceList {
            devices: None,
            local_name: daemon.0.local_device_name(),
        };
        Self { daemon, devices }
    }

    fn update(&mut self, message: Message) {
        self.devices.update(message);
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::run_with(self.daemon.clone(), watch)
    }

    fn view(&self) -> Element<'_, Message> {
        self.devices.view()
    }
}

/// The home screen: this computer's paired devices.
struct DeviceList {
    /// `None` until the first snapshot arrives.
    devices: Option<Vec<DeviceSnapshot>>,
    local_name: String,
}

impl DeviceList {
    fn update(&mut self, message: Message) {
        match message {
            Message::Snapshot {
                devices,
                local_name,
            } => {
                self.devices = Some(devices);
                self.local_name = local_name;
            }
            Message::Event(event) => {
                let Some(devices) = &mut self.devices else {
                    return;
                };
                match event.event {
                    EventData::DeviceDiscovered(device)
                    | EventData::DeviceConnected(device)
                    | EventData::DeviceUpdated(device)
                    | EventData::DeviceDisconnected(device) => {
                        devices.retain(|known| known.device_id != device.device_id);
                        devices.push(device);
                    }
                    EventData::DeviceForgotten(device) => {
                        devices.retain(|known| known.device_id != device.device_id);
                    }
                    EventData::SettingsChanged(settings) => {
                        self.local_name = settings.device_name;
                    }
                    _ => {}
                }
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let header = column![
            text("Devices").size(26).font(Font {
                weight: font::Weight::Semibold,
                ..Font::DEFAULT
            }),
            row![
                lucide::monitor().size(13).style(text::secondary),
                text(&self.local_name).size(13).style(text::secondary),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ]
        .spacing(4);

        let body: Element<'_, Message> = match &self.devices {
            None => placeholder(lucide::loader().into(), "Starting…", None),
            Some(devices) => {
                let mut paired: Vec<_> = devices.iter().filter(|d| d.paired).collect();
                if paired.is_empty() {
                    placeholder(
                        lucide::monitor_smartphone().size(40).into(),
                        "No paired devices yet",
                        Some("Devices you pair with appear here."),
                    )
                } else {
                    // Connected first, then by name.
                    paired.sort_by_key(|d| (!is_connected(d), d.device_name.to_lowercase()));
                    scrollable(column(paired.into_iter().map(device_card)).spacing(8))
                        .spacing(6)
                        .height(Length::Fill)
                        .into()
                }
            }
        };

        container(column![header, body].spacing(20))
            .padding(20)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

/// The core's device list: a snapshot, then events, and a fresh snapshot
/// whenever the receiver falls behind. Subscribing before taking the
/// snapshot means no change is missed; events already reflected in it are
/// replayed harmlessly, since each carries the device's whole state.
fn watch(daemon: &Daemon) -> impl Stream<Item = Message> + use<> {
    let core = daemon.0.clone();
    iced::stream::channel(64, async move |mut output| {
        loop {
            let mut events = core.subscribe();
            let devices = core.devices().unwrap_or_default();
            let local_name = core.local_device_name();
            if output
                .send(Message::Snapshot {
                    devices,
                    local_name,
                })
                .await
                .is_err()
            {
                return;
            }
            loop {
                match events.recv().await {
                    Ok(event) => {
                        if output.send(Message::Event(Box::new(event))).await.is_err() {
                            return;
                        }
                    }
                    Err(RecvError::Lagged(_)) => break,
                    Err(RecvError::Closed) => return,
                }
            }
        }
    })
}

fn device_card(device: &DeviceSnapshot) -> Element<'_, Message> {
    let connected = is_connected(device);

    let badge = container(device_icon(device.device_type).size(20))
        .center(40)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();
            let pair = if connected {
                palette.primary.weak
            } else {
                palette.background.strong
            };
            container::Style {
                background: Some(Background::Color(pair.color)),
                text_color: Some(pair.text),
                border: Border::default().rounded(20),
                ..container::Style::default()
            }
        });

    let mut status = row![
        status_dot(device.reachability),
        text(reachability_label(device.reachability))
            .size(13)
            .style(text::secondary)
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    if let Some(battery) = Battery::of(device) {
        let icon = if battery.charging {
            lucide::battery_charging()
        } else if battery.charge <= 15 {
            lucide::battery_low()
        } else if battery.charge <= 60 {
            lucide::battery_medium()
        } else {
            lucide::battery_full()
        };
        status = status.push(Space::new().width(6)).push(
            row![
                icon.size(14).style(text::secondary),
                text(format!("{}%", battery.charge))
                    .size(13)
                    .style(text::secondary),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }

    let card = row![
        badge,
        column![text(&device.device_name).size(15), status].spacing(3),
        space::horizontal(),
    ]
    .spacing(14)
    .align_y(Alignment::Center);

    container(card)
        .padding([12, 14])
        .width(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(Background::Color(palette.background.weakest.color)),
                border: Border::default()
                    .rounded(10)
                    .width(1)
                    .color(palette.background.weak.color),
                ..container::Style::default()
            }
        })
        .into()
}

fn status_dot<'a>(reachability: DeviceReachability) -> Element<'a, Message> {
    container(Space::new())
        .width(8)
        .height(8)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();
            let color: Color = match reachability {
                DeviceReachability::Connected => palette.success.base.color,
                DeviceReachability::Discovered => palette.warning.base.color,
                DeviceReachability::Unavailable => palette.background.strongest.color,
            };
            container::Style {
                background: Some(Background::Color(color)),
                border: Border::default().rounded(4),
                ..container::Style::default()
            }
        })
        .into()
}

fn placeholder<'a>(
    icon: Element<'a, Message>,
    title: &'a str,
    detail: Option<&'a str>,
) -> Element<'a, Message> {
    let mut content = column![icon, text(title).size(16)]
        .spacing(10)
        .align_x(Alignment::Center);
    if let Some(detail) = detail {
        content = content.push(text(detail).size(13).style(text::secondary));
    }
    container(content).center(Length::Fill).into()
}

fn device_icon<'a>(device_type: DeviceType) -> iced::widget::Text<'a> {
    match device_type {
        DeviceType::Desktop => lucide::monitor(),
        DeviceType::Laptop => lucide::laptop(),
        DeviceType::Phone => lucide::smartphone(),
        DeviceType::Tablet => lucide::tablet(),
        DeviceType::Tv => lucide::tv(),
    }
}

fn is_connected(device: &DeviceSnapshot) -> bool {
    device.reachability == DeviceReachability::Connected
}

fn reachability_label(reachability: DeviceReachability) -> &'static str {
    match reachability {
        DeviceReachability::Connected => "Connected",
        DeviceReachability::Discovered => "Nearby",
        DeviceReachability::Unavailable => "Offline",
    }
}

/// The battery plugin's state for a device, if it reported one.
struct Battery {
    charge: i64,
    charging: bool,
}

impl Battery {
    fn of(device: &DeviceSnapshot) -> Option<Self> {
        let state = device.plugins.get("battery")?;
        Some(Self {
            charge: state.get("charge")?.as_i64()?,
            charging: state.get("charging")?.as_bool()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use iced::{Settings, Theme};
    use serde_json::json;

    use super::*;

    fn device(
        name: &str,
        device_type: DeviceType,
        reachability: DeviceReachability,
        battery: Option<(i64, bool)>,
    ) -> DeviceSnapshot {
        let mut plugins = BTreeMap::new();
        if let Some((charge, charging)) = battery {
            plugins.insert(
                "battery".into(),
                json!({"charge": charge, "charging": charging}),
            );
        }
        DeviceSnapshot {
            device_id: format!("{name:0<32}"),
            device_name: name.into(),
            device_type,
            protocol_version: 8,
            incoming_capabilities: vec![],
            outgoing_capabilities: vec![],
            reachability,
            paired: true,
            pairing: false,
            last_seen_at: 0,
            plugins,
        }
    }

    /// Renders the device list headlessly to `$SNAPSHOT_DIR/<theme>.png`,
    /// to look at the UI without a display. Skipped without the variable.
    #[test]
    fn snapshot_device_list() {
        let Ok(directory) = std::env::var("SNAPSHOT_DIR") else {
            return;
        };
        let list = DeviceList {
            devices: Some(vec![
                device(
                    "Pixel 8a",
                    DeviceType::Phone,
                    DeviceReachability::Connected,
                    Some((82, false)),
                ),
                device(
                    "Galaxy Tab S9",
                    DeviceType::Tablet,
                    DeviceReachability::Connected,
                    Some((45, true)),
                ),
                device(
                    "Work laptop",
                    DeviceType::Laptop,
                    DeviceReachability::Unavailable,
                    None,
                ),
                device(
                    "Living room TV",
                    DeviceType::Tv,
                    DeviceReachability::Unavailable,
                    None,
                ),
            ]),
            local_name: "Spike Mac".into(),
        };
        for (name, theme) in [("light", Theme::Light), ("dark", Theme::Dark)] {
            let mut ui = iced_test::simulator::Simulator::with_size(
                Settings {
                    fonts: vec![iced_fonts::LUCIDE_FONT_BYTES.into()],
                    ..Settings::default()
                },
                (440.0, 620.0),
                list.view(),
            );
            let snapshot = ui.snapshot(&theme).expect("snapshot renders");
            let path = std::path::Path::new(&directory).join(name);
            let _ = std::fs::remove_file(path.with_extension("png"));
            assert!(snapshot.matches_image(&path).expect("snapshot saves"));
        }
    }
}
