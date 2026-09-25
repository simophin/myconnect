//! The home page: this computer's paired devices.

use iced::{
    Alignment, Background, Border, Color, Element, Length, Theme,
    widget::{Space, column, container, row, scrollable, space, text},
};
use iced_fonts::lucide;

use crate::{
    core::{DeviceReachability, DeviceSnapshot},
    protocol::DeviceType,
    ui::{
        plugin::ErasedUiPlugin,
        store::{Load, Store},
        widgets,
    },
};

/// The home page, from what `store` holds, with each device's status from
/// `plugins`. `retry` takes a fresh snapshot after a failed one.
pub fn view<'a, Message: Clone + 'a>(
    store: &'a Store,
    plugins: &'a [Box<dyn ErasedUiPlugin>],
    retry: Message,
) -> Element<'a, Message> {
    let mut header = column![widgets::page_header("Devices", None, vec![])].spacing(4);
    if let Some(settings) = store.settings().loaded() {
        header = header.push(
            row![
                lucide::monitor().size(13).style(text::secondary),
                text(&settings.device_name).size(13).style(text::secondary),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }

    let body: Element<'a, Message> = match store.paired_devices() {
        Load::Loading => widgets::loading("Loading devices…"),
        Load::Failed(error) => widgets::error_view(error, Some(retry)),
        Load::Loaded(paired) if paired.is_empty() => widgets::empty_state(
            lucide::monitor_smartphone,
            "No paired devices yet",
            Some("Devices you pair with appear here."),
        ),
        Load::Loaded(mut paired) => {
            // Connected first; the store sorts by name.
            paired.sort_by_key(|device| !is_connected(device));
            let cards = paired
                .into_iter()
                .map(|device| device_card(device, plugins));
            scrollable(column(cards).spacing(8))
                .spacing(6)
                .height(Length::Fill)
                .into()
        }
    };

    widgets::page(header, body)
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
        let store = testing::store(
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
        testing::snapshot("devices", (440.0, 620.0), || view(&store, &plugins, ()));
    }
}
