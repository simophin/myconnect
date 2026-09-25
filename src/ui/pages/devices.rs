//! The home page: this computer's paired devices.

use iced::{
    Alignment, Background, Border, Color, Element, Length, Theme,
    widget::{Space, column, container, row, scrollable, space, text},
};
use iced_fonts::lucide;

use crate::{
    core::{DeviceReachability, DeviceSnapshot, EventData},
    protocol::DeviceType,
    ui::{plugin::ErasedUiPlugin, sync::Update, widgets},
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

    /// The page once `devices` have loaded, to test it.
    #[cfg(test)]
    pub(crate) fn loaded(local_name: &str, devices: Vec<DeviceSnapshot>) -> Self {
        Self {
            devices: Some(devices),
            local_name: local_name.into(),
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

    /// A known device, by id.
    pub fn device(&self, device_id: &str) -> Option<&DeviceSnapshot> {
        self.devices
            .as_ref()?
            .iter()
            .find(|device| device.device_id == device_id)
    }

    /// The page, with each device's status from `plugins`.
    pub fn view<'a, Message: Clone + 'a>(
        &'a self,
        plugins: &'a [Box<dyn ErasedUiPlugin>],
    ) -> Element<'a, Message> {
        let this_computer = row![
            lucide::monitor().size(13).style(text::secondary),
            text(&self.local_name).size(13).style(text::secondary),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let body: Element<'a, Message> = match &self.devices {
            None => widgets::loading("Loading devices…"),
            Some(devices) => {
                let mut paired: Vec<_> = devices.iter().filter(|d| d.paired).collect();
                if paired.is_empty() {
                    widgets::empty_state(
                        lucide::monitor_smartphone,
                        "No paired devices yet",
                        Some("Devices you pair with appear here."),
                    )
                } else {
                    // Connected first, then by name.
                    paired.sort_by_key(|d| (!is_connected(d), d.device_name.to_lowercase()));
                    let cards = paired
                        .into_iter()
                        .map(|device| device_card(device, plugins));
                    scrollable(column(cards).spacing(8))
                        .spacing(6)
                        .height(Length::Fill)
                        .into()
                }
            }
        };

        widgets::page(
            column![widgets::page_header("Devices", None, vec![]), this_computer].spacing(4),
            body,
        )
    }
}

fn device_card<'a, Message: 'a>(
    device: &'a DeviceSnapshot,
    plugins: &'a [Box<dyn ErasedUiPlugin>],
) -> Element<'a, Message> {
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

    let mut status_row = row![
        status_dot(device.reachability),
        text(reachability_label(device.reachability))
            .size(13)
            .style(text::secondary)
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    for status in plugins
        .iter()
        .filter_map(|plugin| plugin.device_status(device))
    {
        status_row = status_row.push(Space::new().width(6)).push(
            row![
                (status.icon)().size(14).style(text::secondary),
                text(status.label).size(13).style(text::secondary),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }

    let card = row![
        badge,
        column![text(&device.device_name).size(15), status_row].spacing(3),
        space::horizontal(),
    ]
    .spacing(14)
    .align_y(Alignment::Center);

    widgets::card(card).into()
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

#[cfg(test)]
mod tests {
    use iced_fonts::lucide;
    use serde_json::json;

    use super::*;
    use crate::ui::{
        plugin::{Command, DeviceStatus, UiContext, UiPlugin},
        testing,
    };

    /// A feature that shows a device's signal, if it reports one, as a
    /// stand-in for any plugin's status.
    struct Signal;

    impl UiPlugin for Signal {
        type Message = ();

        fn id(&self) -> &'static str {
            "signal"
        }

        fn device_status(&self, device: &DeviceSnapshot) -> Option<DeviceStatus> {
            let bars = device.plugins.get("signal")?.as_u64()?;
            Some(DeviceStatus {
                icon: lucide::signal,
                label: format!("{bars} bars"),
            })
        }

        fn update(&mut self, _ctx: &UiContext, _message: ()) -> Command<()> {
            Command::none()
        }
    }

    fn device(
        name: &str,
        device_type: DeviceType,
        reachability: DeviceReachability,
        signal: Option<u64>,
    ) -> DeviceSnapshot {
        let mut device = testing::device(name);
        device.device_type = device_type;
        device.reachability = reachability;
        if let Some(bars) = signal {
            device.plugins.insert("signal".into(), json!(bars));
        }
        device
    }

    #[test]
    fn snapshot_device_list() {
        let list = DeviceList::loaded(
            "Demo desktop",
            vec![
                device(
                    "Pixel 8a",
                    DeviceType::Phone,
                    DeviceReachability::Connected,
                    Some(4),
                ),
                device(
                    "Galaxy Tab S9",
                    DeviceType::Tablet,
                    DeviceReachability::Connected,
                    Some(2),
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
            ],
        );
        let plugins: Vec<Box<dyn ErasedUiPlugin>> = vec![Box::new(Signal)];
        testing::snapshot("devices", (440.0, 620.0), || list.view::<()>(&plugins));
    }
}
