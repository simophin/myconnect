//! The desktop UI, in iced (`gui` feature).
//!
//! The UI runs in the daemon's process and talks to the core directly:
//! snapshots from [`Core`], events from [`Core::subscribe`], and actions
//! through typed Rust functions, never the HTTP API. The daemon still serves
//! that API, so the CLI can drive and inspect the instance the UI shows.
//!
//! This module is the UI core. It never names a feature: each feature's UI
//! half lives in `src/plugins/<name>/ui.rs` and plugs in through
//! [`plugin::ErasedUiPlugin`]. See `docs/adr/0001-native-ui-in-iced.md`.

pub mod demo;
pub mod pages;
pub mod plugin;
pub mod sync;
#[cfg(test)]
mod testing;

use std::cell::RefCell;

use anyhow::{Context, Result};
use iced::{Element, Size, Subscription, Task, window};

use crate::{core::Core, daemon::RunningService};
use pages::devices::DeviceList;
use plugin::ErasedUiPlugin;

/// How the UI runs, besides the service it shows.
pub struct UiOptions {
    /// The daemon's runtime. Work that touches the daemon's sockets runs
    /// here, not on iced's executor.
    pub runtime: tokio::runtime::Handle,
    /// Fill the core with made-up devices ([`demo`]).
    pub demo: bool,
}

/// Show the UI for `service` until the window closes.
pub fn run(
    service: &RunningService,
    options: UiOptions,
    plugins: Vec<Box<dyn ErasedUiPlugin>>,
) -> Result<()> {
    let core = service.core().clone();
    if options.demo {
        options.runtime.spawn(demo::run(core.clone()));
    }
    // iced asks for the state through a `Fn`; it boots once.
    let plugins = RefCell::new(Some(plugins));
    iced::daemon(
        move || App::boot(core.clone(), plugins.take().unwrap_or_default()),
        App::update,
        App::view,
    )
    .title("MyConnect")
    .subscription(App::subscription)
    .font(iced_fonts::LUCIDE_FONT_BYTES)
    .run()
    .context("UI failed")
}

#[derive(Debug, Clone)]
enum Message {
    Sync(sync::Update),
    WindowOpened,
    WindowClosed(window::Id),
}

struct App {
    core: Core,
    /// The main window.
    window: window::Id,
    devices: DeviceList,
}

impl App {
    fn boot(core: Core, plugins: Vec<Box<dyn ErasedUiPlugin>>) -> (Self, Task<Message>) {
        let ids: Vec<_> = plugins.iter().map(|plugin| plugin.id()).collect();
        tracing::debug!(plugins = ?ids, "UI plugins");
        let (window, open) = window::open(window::Settings {
            size: Size::new(440.0, 620.0),
            ..window::Settings::default()
        });
        let app = Self {
            devices: DeviceList::new(core.local_device_name()),
            core,
            window,
        };
        (app, open.map(|_| Message::WindowOpened))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Sync(update) => {
                self.devices.update(update);
                Task::none()
            }
            Message::WindowOpened => Task::none(),
            // Closing the window quits, until the tray can keep the app
            // running without one.
            Message::WindowClosed(id) if id == self.window => iced::exit(),
            Message::WindowClosed(_) => Task::none(),
        }
    }

    fn view(&self, _window: window::Id) -> Element<'_, Message> {
        self.devices.view()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            sync::watch(&self.core).map(Message::Sync),
            window::close_events().map(Message::WindowClosed),
        ])
    }
}
