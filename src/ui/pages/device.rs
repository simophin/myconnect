//! One device: what it is, what can be done with it (each plugin's
//! actions), its recent transfers, and Unpair.

use iced::{
    Alignment, Background, Border, Element, Length, Theme,
    widget::{button, column, container, row, scrollable, space, text},
};
use iced_fonts::lucide;

use super::{
    devices::{device_icon, is_connected, status_row},
    transfers::transfer_row,
};
use crate::{
    core::DeviceSnapshot,
    protocol::DeviceType,
    ui::{
        plugin::{DeviceAction, ErasedUiPlugin, PluginMessage},
        route::Route,
        store::Store,
        widgets,
    },
};

/// How many of the device's transfers the page lists.
const RECENT_TRANSFERS: usize = 5;

/// The page of the device `device_id`, from what `store` holds, with each
/// plugin's status and actions. `navigate` makes the message that opens a
/// page (Back goes to the device list), `plugin` wraps an action's message,
/// and `unpair` asks to unpair the device; `unpairing` disables it while an
/// unpair runs.
pub fn view<'a, Message: Clone + 'a>(
    store: &'a Store,
    plugins: &'a [Box<dyn ErasedUiPlugin>],
    device_id: &str,
    unpairing: bool,
    navigate: impl Fn(Route) -> Message,
    plugin: impl Fn(PluginMessage) -> Message,
    unpair: impl Fn(&DeviceSnapshot) -> Message,
) -> Element<'a, Message> {
    let back = Some(navigate(Route::Devices));
    let Some(device) = store.device(device_id) else {
        return widgets::page(
            widgets::page_header("Device", back, vec![]),
            widgets::empty_state(
                lucide::circle_alert,
                "This device is no longer known.",
                None,
                None,
            ),
        );
    };

    let mut content = column![
        summary(device, plugins),
        facts(device),
        actions(
            plugins
                .iter()
                .flat_map(|each| each.device_actions(device))
                .map(|action| action.map(&plugin)),
        ),
    ]
    .spacing(16);

    let transfers = store.transfers(Some(device_id)).into_loaded();
    if let Some(transfers) = transfers.filter(|transfers| !transfers.is_empty()) {
        let rows = transfers
            .into_iter()
            .take(RECENT_TRANSFERS)
            .map(|transfer| transfer_row(transfer, false));
        content = content.push(
            column![
                row![
                    text("Recent transfers").font(widgets::semibold()),
                    space::horizontal(),
                    widgets::link_button("See all", navigate(Route::Transfers)),
                ]
                .align_y(Alignment::Center),
                widgets::card(column(rows).spacing(14)),
            ]
            .spacing(6),
        );
    }

    content = content.push(unpair_button((!unpairing).then(|| unpair(device))));

    widgets::page(
        widgets::page_header(&device.device_name, back, vec![]),
        scrollable(content.padding(iced::Padding::default().right(12)))
            .spacing(4)
            .height(Length::Fill),
    )
}

/// A large icon, the name, and how the device is: reachability and each
/// plugin's status.
fn summary<'a, Message: 'a>(
    device: &'a DeviceSnapshot,
    plugins: &'a [Box<dyn ErasedUiPlugin>],
) -> Element<'a, Message> {
    let connected = is_connected(device);
    let badge = container(device_icon(device.device_type).size(30))
        .center(56)
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
                border: Border::default().rounded(28),
                ..container::Style::default()
            }
        });
    row![
        badge,
        column![
            text(&device.device_name).size(18).font(widgets::semibold()),
            status_row(device, plugins),
        ]
        .spacing(4),
    ]
    .spacing(16)
    .align_y(Alignment::Center)
    .into()
}

/// The device's id, type and protocol version, selectable so they can be
/// copied.
fn facts<'a, Message: Clone + 'a>(device: &'a DeviceSnapshot) -> Element<'a, Message> {
    let fact = |label: &'static str, value: &str| {
        column![
            text(label).size(12).style(text::secondary),
            widgets::selectable_text(value)
        ]
        .spacing(2)
    };
    widgets::card(
        column![
            fact("Device ID", &device.device_id),
            fact("Type", type_name(device.device_type)),
            fact("Protocol version", &device.protocol_version.to_string()),
        ]
        .spacing(10),
    )
    .into()
}

/// A device type as the protocol names it, as the Flutter app showed it.
fn type_name(device_type: DeviceType) -> &'static str {
    match device_type {
        DeviceType::Desktop => "desktop",
        DeviceType::Laptop => "laptop",
        DeviceType::Phone => "phone",
        DeviceType::Tablet => "tablet",
        DeviceType::Tv => "tv",
    }
}

/// The plugins' actions as buttons, wrapping onto more lines as needed. A
/// disabled action is shown, but can't be pressed.
fn actions<'a, Message: Clone + 'a>(
    actions: impl Iterator<Item = DeviceAction<Message>>,
) -> Element<'a, Message> {
    let buttons = actions.map(|action| {
        let content = row![(action.icon)().size(16), text(action.label)]
            .spacing(8)
            .align_y(Alignment::Center);
        button(content)
            .padding([8, 14])
            .style(widgets::tonal)
            .on_press_maybe(action.enabled.then_some(action.message))
            .into()
    });
    row(buttons).spacing(8).wrap().vertical_spacing(8).into()
}

/// Unpair, drawn as a destructive outlined button; disabled without a
/// message.
fn unpair_button<'a, Message: Clone + 'a>(on_press: Option<Message>) -> Element<'a, Message> {
    let content = row![lucide::link_two_off().size(16), text("Unpair")]
        .spacing(8)
        .align_y(Alignment::Center);
    let button = button(content)
        .padding([8, 14])
        .style(|theme: &Theme, status| {
            let palette = theme.extended_palette();
            let color = if status == button::Status::Disabled {
                palette.background.strong.color
            } else {
                palette.danger.base.color
            };
            let background = match status {
                button::Status::Hovered => Some(palette.danger.base.color.scale_alpha(0.08)),
                button::Status::Pressed => Some(palette.danger.base.color.scale_alpha(0.16)),
                _ => None,
            };
            button::Style {
                background: background.map(Background::Color),
                text_color: color,
                border: Border::default().rounded(8).width(1).color(color),
                ..button::Style::default()
            }
        })
        .on_press_maybe(on_press);
    container(button).padding([8, 0]).into()
}

#[cfg(test)]
mod tests {
    use iced_test::simulator::Simulator;

    use super::*;
    use crate::{
        core::{DeviceReachability, TransferDirection, TransferStatus},
        ui::{
            pages::transfers::tests::transfer,
            plugin::{Command, DeviceStatus, UiContext, UiPlugin},
            store::Snapshot,
            testing,
        },
    };

    /// Stands in for any plugin with a status and actions: "Wave" when the
    /// device takes waves, always listed; "Hug" listed only for phones.
    struct Waver;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum WaveMessage {
        Wave(String),
        Hug(String),
    }

    const WAVE: &str = "example.wave";

    impl UiPlugin for Waver {
        type Message = WaveMessage;

        fn id(&self) -> &'static str {
            "waver"
        }

        fn device_status(&self, _device: &DeviceSnapshot) -> Option<DeviceStatus> {
            Some(DeviceStatus {
                icon: lucide::hand,
                label: "Waving".into(),
            })
        }

        fn device_actions(&self, device: &DeviceSnapshot) -> Vec<DeviceAction<WaveMessage>> {
            let mut actions = vec![DeviceAction {
                id: "wave",
                label: "Wave".into(),
                icon: lucide::hand,
                enabled: is_connected(device)
                    && device.incoming_capabilities.iter().any(|c| c == WAVE),
                visible_in_tray: true,
                message: WaveMessage::Wave(device.device_id.clone()),
            }];
            if device.device_type == DeviceType::Phone {
                actions.push(DeviceAction {
                    id: "hug",
                    label: "Hug".into(),
                    icon: lucide::heart,
                    enabled: true,
                    visible_in_tray: false,
                    message: WaveMessage::Hug(device.device_id.clone()),
                });
            }
            actions
        }

        fn update(&mut self, _ctx: &UiContext, _message: WaveMessage) -> Command<WaveMessage> {
            Command::none()
        }
    }

    /// What the page asks for, in tests.
    #[derive(Debug, Clone, PartialEq)]
    enum Asked {
        Go(Route),
        Plugin(String),
        Unpair(String),
    }

    fn plugins() -> Vec<Box<dyn ErasedUiPlugin>> {
        vec![Box::new(Waver)]
    }

    fn page<'a>(
        store: &'a Store,
        plugins: &'a [Box<dyn ErasedUiPlugin>],
        device_id: &str,
        unpairing: bool,
    ) -> Element<'a, Asked> {
        view(
            store,
            plugins,
            device_id,
            unpairing,
            Asked::Go,
            |message| Asked::Plugin(format!("{message:?}")),
            |device| Asked::Unpair(device.device_id.clone()),
        )
    }

    fn pixel(capabilities: &[&str], reachability: DeviceReachability) -> DeviceSnapshot {
        let mut device = testing::device("Pixel");
        device.incoming_capabilities = capabilities.iter().map(|c| (*c).into()).collect();
        device.reachability = reachability;
        device
    }

    /// What clicking `target` on the page of `device` asks for.
    fn click(store: &Store, device: &DeviceSnapshot, target: &str) -> Vec<Asked> {
        let plugins = plugins();
        let mut ui = Simulator::new(page(store, &plugins, &device.device_id, false));
        ui.click(target).expect("the target is on the page");
        ui.into_messages().collect()
    }

    #[test]
    fn shows_the_device_its_status_and_facts() {
        let device = pixel(&[], DeviceReachability::Connected);
        let store = testing::store("Desk", vec![device.clone()]);
        let plugins = plugins();
        let mut ui = Simulator::new(page(&store, &plugins, &device.device_id, false));
        for shown in [
            "Connected",
            "Waving",
            "Device ID",
            "Type",
            "phone",
            "Protocol version",
            "8",
            "Wave",
            "Hug",
            "Unpair",
        ] {
            assert!(ui.find(shown).is_ok(), "{shown} is shown");
        }
        assert!(ui.find("Recent transfers").is_err(), "no transfers yet");
    }

    #[test]
    fn a_device_that_is_gone_says_so() {
        let store = testing::store("Desk", Vec::new());
        let plugins = plugins();
        let mut ui = Simulator::new(page(&store, &plugins, "gone", false));
        assert!(ui.find("This device is no longer known.").is_ok());
        ui.click(iced::widget::Id::from("Back")).unwrap();
        assert_eq!(
            ui.into_messages().collect::<Vec<_>>(),
            [Asked::Go(Route::Devices)]
        );
    }

    #[test]
    fn an_action_can_be_chosen_only_while_it_is_enabled() {
        let disabled = pixel(&[], DeviceReachability::Connected);
        let store = testing::store("Desk", vec![disabled.clone()]);
        assert!(click(&store, &disabled, "Wave").is_empty());

        let enabled = pixel(&[WAVE], DeviceReachability::Connected);
        let store = testing::store("Desk", vec![enabled.clone()]);
        let [Asked::Plugin(message)] = &click(&store, &enabled, "Wave")[..] else {
            panic!("the action's message");
        };
        assert!(message.contains("Wave"), "{message}");

        let offline = pixel(&[WAVE], DeviceReachability::Unavailable);
        let store = testing::store("Desk", vec![offline.clone()]);
        assert!(click(&store, &offline, "Wave").is_empty());
    }

    #[test]
    fn an_action_a_plugin_doesnt_list_isnt_shown() {
        let mut laptop = pixel(&[], DeviceReachability::Connected);
        laptop.device_type = DeviceType::Laptop;
        let store = testing::store("Desk", vec![laptop.clone()]);
        let plugins = plugins();
        let mut ui = Simulator::new(page(&store, &plugins, &laptop.device_id, false));
        assert!(ui.find("Hug").is_err());
    }

    #[test]
    fn unpair_asks_and_is_disabled_while_it_runs() {
        let device = pixel(&[], DeviceReachability::Connected);
        let store = testing::store("Desk", vec![device.clone()]);
        assert_eq!(
            click(&store, &device, "Unpair"),
            [Asked::Unpair(device.device_id.clone())]
        );
        let plugins = plugins();
        let mut ui = Simulator::new(page(&store, &plugins, &device.device_id, true));
        ui.click("Unpair").unwrap();
        assert!(ui.into_messages().next().is_none());
    }

    fn with_transfers(device: &DeviceSnapshot, count: u64) -> Store {
        let mut other = transfer(
            "someone-else.txt",
            TransferDirection::Incoming,
            TransferStatus::Completed,
            count + 1,
        );
        other.device_id = "another".into();
        let mut transfers: Vec<_> = (0..count)
            .map(|index| {
                transfer(
                    &format!("photo-{index}.jpg"),
                    TransferDirection::Outgoing,
                    TransferStatus::Completed,
                    index,
                )
            })
            .collect();
        transfers.push(other);
        let mut store = testing::store("Desk", vec![device.clone()]);
        let settings = store.settings().loaded().cloned();
        store.apply_snapshot(Snapshot {
            devices: Ok(vec![device.clone()]),
            pairings: Ok(Vec::new()),
            transfers,
            settings: settings.ok_or_else(String::new),
        });
        store
    }

    #[test]
    fn lists_the_five_newest_transfers_and_links_to_all() {
        let device = pixel(&[], DeviceReachability::Connected);
        let store = with_transfers(&device, 7);
        let plugins = plugins();
        let mut ui = Simulator::new(page(&store, &plugins, &device.device_id, false));
        for index in 2..7 {
            assert!(ui.find(format!("photo-{index}.jpg")).is_ok(), "{index}");
        }
        for hidden in ["photo-1.jpg", "photo-0.jpg", "someone-else.txt"] {
            assert!(ui.find(hidden).is_err(), "{hidden} isn't listed");
        }
        assert!(ui.find("To Pixel · 3.0 MB").is_err(), "no device name");
        assert_eq!(
            click(&store, &device, "See all"),
            [Asked::Go(Route::Transfers)]
        );
    }

    #[test]
    fn snapshot_device_page() {
        let mut device = pixel(&[WAVE], DeviceReachability::Connected);
        device.device_id = "740bd4b9b4184ee497d6caf1da8151be".into();
        let mut store = with_transfers(&device, 0);
        let transfers = vec![
            transfer(
                "holiday.jpg",
                TransferDirection::Incoming,
                TransferStatus::Transferring,
                3,
            ),
            transfer(
                "notes.pdf",
                TransferDirection::Outgoing,
                TransferStatus::Completed,
                2,
            ),
            transfer(
                "song.mp3",
                TransferDirection::Outgoing,
                TransferStatus::Failed,
                1,
            ),
        ]
        .into_iter()
        .map(|mut transfer| {
            transfer.device_id.clone_from(&device.device_id);
            transfer
        })
        .collect();
        let settings = store.settings().loaded().cloned();
        store.apply_snapshot(Snapshot {
            devices: Ok(vec![device.clone()]),
            pairings: Ok(Vec::new()),
            transfers,
            settings: settings.ok_or_else(String::new),
        });
        let plugins = plugins();
        testing::snapshot("device", (440.0, 720.0), || {
            page(&store, &plugins, &device.device_id, false)
        });
        let mut offline = device.clone();
        offline.reachability = DeviceReachability::Unavailable;
        let offline_store = testing::store("Desk", vec![offline.clone()]);
        testing::snapshot("device-offline", (440.0, 560.0), || {
            page(&offline_store, &plugins, &offline.device_id, true)
        });
        let gone = testing::store("Desk", Vec::new());
        testing::snapshot("device-gone", (440.0, 320.0), || {
            page(&gone, &plugins, "gone", false)
        });
    }
}
