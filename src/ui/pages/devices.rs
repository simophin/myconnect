//! The home page: this computer's paired devices.

use iced::{
    Alignment, Background, Border, Color, Element, Length, Theme,
    widget::{Space, button, column, container, row, scrollable, space, text},
};
use iced_fonts::lucide;

use crate::{
    core::{DeviceReachability, DeviceSnapshot},
    protocol::DeviceType,
    ui::{
        plugin::ErasedUiPlugin,
        route::Route,
        store::{Load, Store},
        widgets::{self, HeaderAction},
    },
};

/// The home page, from what `store` holds, with each device's status from
/// `plugins`. `navigate` makes the message that opens a page, and `retry`
/// takes a fresh snapshot after a failed one. The card of `drop_target`,
/// if any, shows that files dropped now go to it.
pub fn view<'a, Message: Clone + 'a>(
    store: &'a Store,
    plugins: &'a [Box<dyn ErasedUiPlugin>],
    drop_target: Option<&str>,
    navigate: impl Fn(Route) -> Message,
    retry: Message,
) -> Element<'a, Message> {
    let title = widgets::page_header(
        "Devices",
        None,
        vec![
            HeaderAction::new(lucide::settings, "Settings", navigate(Route::Settings)),
            HeaderAction::new(
                lucide::arrow_up_down,
                "Transfers",
                navigate(Route::Transfers),
            ),
        ],
    );
    let this_computer: Element<'a, Message> = match store.settings().loaded() {
        Some(settings) => row![
            lucide::monitor().size(13).style(text::secondary),
            text(format!("This computer: {}", settings.device_name))
                .size(13)
                .style(text::secondary)
                .wrapping(text::Wrapping::None),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into(),
        None => Space::new().into(),
    };
    let add = button(
        row![lucide::plus().size(16), text("Add device")]
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .padding([8, 16])
    .style(widgets::filled)
    .on_press(navigate(Route::AddDevice));
    let header = column![
        title,
        row![
            container(this_computer)
                .padding([0, 4])
                .width(Length::Fill)
                .clip(true),
            add
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    ]
    .spacing(8);

    let body: Element<'a, Message> = match store.paired_devices() {
        Load::Loading => widgets::loading("Loading devices…"),
        Load::Failed(error) => widgets::error_view(error, Some(retry)),
        Load::Loaded(paired) if paired.is_empty() => widgets::empty_state(
            lucide::monitor_smartphone,
            "No paired devices yet",
            None,
            Some(("Find a device to pair", navigate(Route::AddDevice))),
        ),
        Load::Loaded(mut paired) => {
            // Connected first; the store sorts by name.
            paired.sort_by_key(|device| !is_connected(device));
            let cards = paired.into_iter().map(|device| {
                let dropping = drop_target == Some(device.device_id.as_str());
                device_card(
                    device,
                    plugins,
                    dropping,
                    navigate(Route::Device(device.device_id.clone())),
                )
            });
            scrollable(column(cards).spacing(8))
                .spacing(6)
                .height(Length::Fill)
                .into()
        }
    };

    widgets::page(header, body)
}

/// A device's card, which opens it. While files are dragged over it and it
/// takes them (`dropping`), it says so instead of its status.
fn device_card<'a, Message: Clone + 'a>(
    device: &'a DeviceSnapshot,
    plugins: &'a [Box<dyn ErasedUiPlugin>],
    dropping: bool,
    open: Message,
) -> Element<'a, Message> {
    let connected = is_connected(device);

    let icon = if dropping {
        lucide::file_up()
    } else {
        device_icon(device.device_type)
    };
    let badge = container(icon.size(20))
        .center(40)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();
            let pair = if dropping {
                palette.primary.base
            } else if connected {
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

    let status_row = if dropping {
        row![
            text("Drop to send")
                .size(13)
                .font(widgets::semibold())
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.extended_palette().primary.weak.text),
                })
        ]
    } else {
        status_row(device, plugins)
    };

    let content = row![
        badge,
        column![text(&device.device_name).size(15), status_row].spacing(3),
        space::horizontal(),
        lucide::chevron_right().size(18).style(text::secondary),
    ]
    .spacing(14)
    .align_y(Alignment::Center);

    button(content)
        .padding([12, 14])
        .width(Length::Fill)
        .style(move |theme: &Theme, status| card_style(theme, status, dropping))
        .on_press(open)
        .into()
}

/// How reachable `device` is, with a coloured dot, then each plugin's
/// status (battery).
pub fn status_row<'a, Message: 'a>(
    device: &DeviceSnapshot,
    plugins: &[Box<dyn ErasedUiPlugin>],
) -> iced::widget::Row<'a, Message> {
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
    status_row
}

/// A card that reacts to the pointer, and stands out while it is where
/// files would be dropped.
fn card_style(theme: &Theme, status: button::Status, highlighted: bool) -> button::Style {
    let palette = theme.extended_palette();
    let card = widgets::card_style(theme);
    let (background, border) = if highlighted {
        (palette.primary.weak.color, palette.primary.base.color)
    } else {
        let background = match status {
            button::Status::Hovered => palette.background.weak.color,
            button::Status::Pressed => palette.background.strong.color,
            _ => palette.background.weakest.color,
        };
        (background, card.border.color)
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: palette.background.base.text,
        border: card.border.color(border),
        ..button::Style::default()
    }
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

/// The icon for a kind of device.
pub fn device_icon<'a>(device_type: DeviceType) -> iced::widget::Text<'a> {
    match device_type {
        DeviceType::Desktop => lucide::monitor(),
        DeviceType::Laptop => lucide::laptop(),
        DeviceType::Phone => lucide::smartphone(),
        DeviceType::Tablet => lucide::tablet(),
        DeviceType::Tv => lucide::tv(),
    }
}

pub fn is_connected(device: &DeviceSnapshot) -> bool {
    device.reachability == DeviceReachability::Connected
}

/// How reachable a device is, in the Flutter app's words
/// (`shared/widgets.dart`).
pub fn reachability_label(reachability: DeviceReachability) -> &'static str {
    match reachability {
        DeviceReachability::Connected => "Connected",
        DeviceReachability::Discovered => "Nearby",
        DeviceReachability::Unavailable => "Not reachable",
    }
}

#[cfg(test)]
mod tests {
    use iced_fonts::lucide;
    use serde_json::json;

    use iced::widget;
    use iced_test::simulator::Simulator;

    use super::*;
    use crate::ui::{
        plugin::{Command, DeviceStatus, UiContext, UiPlugin},
        store::Snapshot,
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

    /// What the page asks for, in tests.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Asked {
        Go(Route),
        Retry,
    }

    fn page<'a>(
        store: &'a Store,
        plugins: &'a [Box<dyn ErasedUiPlugin>],
        drop_target: Option<&str>,
    ) -> Element<'a, Asked> {
        view(store, plugins, drop_target, Asked::Go, Asked::Retry)
    }

    fn some_devices() -> Vec<DeviceSnapshot> {
        vec![
            device(
                "Pixel 8a",
                DeviceType::Phone,
                DeviceReachability::Connected,
                Some(4),
            ),
            device(
                "galaxy Tab S9",
                DeviceType::Tablet,
                DeviceReachability::Connected,
                Some(2),
            ),
            device(
                "Work laptop",
                DeviceType::Laptop,
                DeviceReachability::Discovered,
                None,
            ),
            device(
                "Living room TV",
                DeviceType::Tv,
                DeviceReachability::Unavailable,
                None,
            ),
        ]
    }

    /// What clicking `target` on the page asks for.
    fn click(
        store: &Store,
        target: impl iced_test::selector::Selector<
            Output: iced_test::selector::Bounded + Clone + Send + Sync + 'static,
        > + Send,
    ) -> Vec<Asked> {
        let plugins: Vec<Box<dyn ErasedUiPlugin>> = vec![Box::new(Signal)];
        let mut ui = Simulator::new(page(store, &plugins, None));
        ui.click(target).expect("the target is on the page");
        ui.into_messages().collect()
    }

    #[test]
    fn lists_paired_devices_only_with_their_status() {
        let mut stranger = device(
            "Stranger",
            DeviceType::Phone,
            DeviceReachability::Connected,
            None,
        );
        stranger.paired = false;
        let mut devices = some_devices();
        devices.push(stranger);
        let store = testing::store("Demo desktop", devices);
        let plugins: Vec<Box<dyn ErasedUiPlugin>> = vec![Box::new(Signal)];
        let mut ui = Simulator::new(page(&store, &plugins, None));

        for shown in [
            "This computer: Demo desktop",
            "Pixel 8a",
            "galaxy Tab S9",
            "Work laptop",
            "Nearby",
            "Not reachable",
            "4 bars",
            "Add device",
        ] {
            assert!(ui.find(shown).is_ok(), "{shown} is shown");
        }
        assert!(ui.find("Stranger").is_err(), "unpaired devices aren't");
        assert!(ui.find("Drop to send").is_err());
    }

    #[test]
    fn connected_devices_come_first_then_by_name() {
        let store = testing::store("Demo desktop", some_devices());
        let plugins: Vec<Box<dyn ErasedUiPlugin>> = vec![];
        let mut ui = Simulator::new(page(&store, &plugins, None));
        let top = |ui: &mut Simulator<'_, Asked>, name: &str| {
            ui.find(name).unwrap().visible_bounds().unwrap().y
        };
        let order: Vec<f32> = ["galaxy Tab S9", "Pixel 8a", "Living room TV", "Work laptop"]
            .into_iter()
            .map(|name| top(&mut ui, name))
            .collect();
        assert!(order.is_sorted(), "{order:?}");
    }

    fn id_of(store: &Store, name: &str) -> String {
        store
            .paired_devices()
            .into_loaded()
            .unwrap()
            .into_iter()
            .find(|device| device.device_name == name)
            .unwrap()
            .device_id
            .clone()
    }

    #[test]
    fn a_card_opens_its_device() {
        let store = testing::store("Demo desktop", some_devices());
        let pixel = id_of(&store, "Pixel 8a");
        assert_eq!(click(&store, "Pixel 8a"), [Asked::Go(Route::Device(pixel))]);
    }

    #[test]
    fn the_header_opens_settings_transfers_and_add_device() {
        let store = testing::store("Demo desktop", some_devices());
        assert_eq!(
            click(&store, widget::Id::from("Settings")),
            [Asked::Go(Route::Settings)]
        );
        assert_eq!(
            click(&store, widget::Id::from("Transfers")),
            [Asked::Go(Route::Transfers)]
        );
        assert_eq!(click(&store, "Add device"), [Asked::Go(Route::AddDevice)]);
    }

    #[test]
    fn with_nothing_paired_it_offers_to_find_a_device() {
        let store = testing::store("Demo desktop", Vec::new());
        assert_eq!(
            click(&store, "Find a device to pair"),
            [Asked::Go(Route::AddDevice)]
        );
    }

    #[test]
    fn a_failed_load_offers_retry() {
        assert_eq!(click(&failed(), "Retry"), [Asked::Retry]);
    }

    fn failed() -> Store {
        let mut store = Store::default();
        store.apply_snapshot(Snapshot {
            devices: Err("The daemon isn't running.".into()),
            pairings: Ok(Vec::new()),
            transfers: Vec::new(),
            settings: Err("The daemon isn't running.".into()),
        });
        store
    }

    #[test]
    fn a_card_under_a_drag_says_drop_to_send() {
        let store = testing::store("Demo desktop", some_devices());
        let plugins: Vec<Box<dyn ErasedUiPlugin>> = vec![Box::new(Signal)];
        let pixel = id_of(&store, "Pixel 8a");
        let mut ui = Simulator::new(page(&store, &plugins, Some(&pixel)));
        assert!(ui.find("Drop to send").is_ok());
        assert!(ui.find("4 bars").is_err(), "the status gives way");
    }

    #[test]
    fn snapshot_device_list() {
        let store = testing::store("Demo desktop", some_devices());
        let plugins: Vec<Box<dyn ErasedUiPlugin>> = vec![Box::new(Signal)];
        let dropping = id_of(&store, "galaxy Tab S9");
        testing::snapshot("devices", (440.0, 620.0), || page(&store, &plugins, None));
        testing::snapshot("devices-drop", (440.0, 620.0), || {
            page(&store, &plugins, Some(&dropping))
        });
        let loading = Store::default();
        testing::snapshot("devices-loading", (440.0, 400.0), || {
            page(&loading, &plugins, None)
        });
        let empty = testing::store("Demo desktop", Vec::new());
        testing::snapshot("devices-empty", (440.0, 400.0), || {
            page(&empty, &plugins, None)
        });
        let failed = failed();
        testing::snapshot("devices-failed", (440.0, 400.0), || {
            page(&failed, &plugins, None)
        });
    }
}
