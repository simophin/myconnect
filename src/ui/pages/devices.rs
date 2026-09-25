//! The home page: this computer's paired devices.

use iced::{
    Alignment, Background, Border, Color, Element, Font, Length, Theme, font,
    widget::{Space, column, container, row, scrollable, space, text},
};
use iced_fonts::lucide;

use crate::{
    core::{DeviceReachability, DeviceSnapshot, EventData},
    protocol::DeviceType,
    ui::sync::Update,
};

/// The home screen: this computer's paired devices.
pub struct DeviceList {
    /// `None` until the first snapshot arrives.
    devices: Option<Vec<DeviceSnapshot>>,
    local_name: String,
}

impl DeviceList {
    pub fn new(local_name: String) -> Self {
        Self {
            devices: None,
            local_name,
        }
    }

    pub fn update(&mut self, update: Update) {
        match update {
            Update::Snapshot {
                devices,
                local_name,
            } => {
                self.devices = Some(devices);
                self.local_name = local_name;
            }
            Update::Event(event) => {
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

    pub fn view<'a, Message: 'a>(&'a self) -> Element<'a, Message> {
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

        let body: Element<'a, Message> = match &self.devices {
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

fn device_card<'a, Message: 'a>(device: &'a DeviceSnapshot) -> Element<'a, Message> {
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

fn status_dot<'a, Message: 'a>(reachability: DeviceReachability) -> Element<'a, Message> {
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

fn placeholder<'a, Message: 'a>(
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

    use serde_json::json;

    use super::*;
    use crate::ui::testing;

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

    #[test]
    fn snapshot_device_list() {
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
            local_name: "Demo desktop".into(),
        };
        testing::snapshot("devices", (440.0, 620.0), || list.view::<()>());
    }
}
