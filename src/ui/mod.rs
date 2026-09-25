//! The desktop UI, in iced (`gui` feature).
//!
//! The UI runs in the daemon's process and talks to the core directly:
//! snapshots from [`Core`], events from [`Core::subscribe`], and actions
//! through typed Rust functions, never the HTTP API. The daemon still serves
//! that API, so the CLI can drive and inspect the instance the UI shows.
//!
//! This module is the UI core. It never names a feature: each feature's UI
//! half lives in `src/plugins/<name>/ui.rs` and plugs in through
//! [`plugin::UiPlugin`]. See `docs/adr/0001-native-ui-in-iced.md`.

pub mod demo;
pub mod pages;
pub mod plugin;
pub mod route;
pub mod sync;
#[cfg(test)]
pub(crate) mod testing;

use std::{cell::RefCell, time::Duration};

use anyhow::{Context, Result};
use iced::{
    Alignment, Background, Border, Element, Length, Size, Subscription, Task, Theme,
    widget::{button, column, container, row, stack, text},
    window,
};

use crate::daemon::RunningService;
use pages::devices::DeviceList;
use plugin::{Command, ErasedUiPlugin, Outcome, PluginMessage, ShellRequest, UiContext};
use route::Route;

/// How long a toast stays up.
const TOAST_DURATION: Duration = Duration::from_secs(4);

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
    // iced asks for the state through a `Fn`; it boots once.
    let plugins = RefCell::new(Some(plugins));
    iced::daemon(
        move || {
            let ctx = UiContext::new(core.clone(), options.runtime.clone());
            App::boot(ctx, plugins.take().unwrap_or_default(), options.demo)
        },
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
    /// A message for the plugin it names.
    Plugin(PluginMessage),
    /// A plugin's request to the shell.
    Shell(ShellRequest<PluginMessage>),
    DismissToast(u64),
    DemoTick(u64),
    WindowOpened,
    Window(window::Id, window::Event),
}

struct App {
    ctx: UiContext,
    /// Every feature's UI half, in `plugins::builtin_with_ui` order.
    plugins: Vec<Box<dyn ErasedUiPlugin>>,
    /// The main window.
    window: window::Id,
    route: Route,
    devices: DeviceList,
    /// Oldest first.
    toasts: Vec<Toast>,
    next_toast: u64,
}

struct Toast {
    id: u64,
    text: String,
    action: Option<(String, Route)>,
}

impl App {
    fn boot(
        ctx: UiContext,
        plugins: Vec<Box<dyn ErasedUiPlugin>>,
        demo: bool,
    ) -> (Self, Task<Message>) {
        let ids: Vec<_> = plugins.iter().map(|plugin| plugin.id()).collect();
        tracing::debug!(plugins = ?ids, "UI plugins");
        let (window, open) = window::open(window::Settings {
            size: Size::new(440.0, 620.0),
            ..window::Settings::default()
        });
        let mut tasks = vec![open.map(|_| Message::WindowOpened)];
        if demo {
            demo::start(ctx.core());
            tasks.push(Task::done(Message::DemoTick(0)));
        }
        let app = Self {
            devices: DeviceList::new(ctx.core().local_device_name()),
            ctx,
            plugins,
            window,
            route: Route::Devices,
            toasts: Vec::new(),
            next_toast: 0,
        };
        (app, Task::batch(tasks))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Sync(update) => {
                let mut tasks = Vec::new();
                if let sync::Update::Event(event) = &update {
                    for plugin in &mut self.plugins {
                        tasks.push(shell_task(plugin.on_event(&self.ctx, event)));
                    }
                }
                self.devices.update(update);
                Task::batch(tasks)
            }
            Message::Plugin(message) => {
                let Some(plugin) = self
                    .plugins
                    .iter_mut()
                    .find(|plugin| plugin.id() == message.plugin())
                else {
                    tracing::warn!(?message, "message for an unknown plugin");
                    return Task::none();
                };
                shell_task(plugin.update(&self.ctx, message))
            }
            Message::Shell(request) => self.handle(request),
            Message::DismissToast(id) => {
                self.toasts.retain(|toast| toast.id != id);
                Task::none()
            }
            Message::DemoTick(tick) => {
                demo::tick(self.ctx.core(), &self.plugins, tick);
                self.after(demo::TICK, Message::DemoTick(tick + 1))
            }
            Message::WindowOpened => Task::none(),
            Message::Window(id, event) if id == self.window => match event {
                window::Event::Focused => {
                    self.ctx.set_window_focused(true);
                    Task::none()
                }
                window::Event::Unfocused => {
                    self.ctx.set_window_focused(false);
                    Task::none()
                }
                // Closing the window quits, until the tray can keep the app
                // running without one.
                window::Event::Closed => iced::exit(),
                _ => Task::none(),
            },
            Message::Window(..) => Task::none(),
        }
    }

    /// Do what a plugin asked of the shell.
    fn handle(&mut self, request: ShellRequest<PluginMessage>) -> Task<Message> {
        match request {
            ShellRequest::Toast { text, action } => self.toast(text, action),
            // Desktop notifications arrive with the tray; until then the
            // window is the only place to say anything.
            ShellRequest::Notify { title, body } => self.toast(format!("{title}: {body}"), None),
            ShellRequest::Navigate(route) => {
                self.route = route;
                Task::none()
            }
            ShellRequest::ShowWindow => window::gain_focus(self.window),
            request @ (ShellRequest::PickFiles { .. }
            | ShellRequest::Confirm { .. }
            | ShellRequest::Prompt { .. }) => {
                tracing::warn!(?request, "the shell can't do this yet");
                Task::none()
            }
        }
    }

    fn toast(&mut self, text: String, action: Option<(String, Route)>) -> Task<Message> {
        let id = self.next_toast;
        self.next_toast += 1;
        self.toasts.push(Toast { id, text, action });
        self.after(TOAST_DURATION, Message::DismissToast(id))
    }

    /// Send `message` after `delay`, timed on the daemon's runtime: iced's
    /// executor has no timer.
    fn after(&self, delay: Duration, message: Message) -> Task<Message> {
        // `sleep` needs the runtime when it is made, not only when polled.
        plugin::on_runtime(self.ctx.runtime(), async move {
            tokio::time::sleep(delay).await;
        })
        .map(move |()| message.clone())
    }

    fn view(&self, _window: window::Id) -> Element<'_, Message> {
        let page = match &self.route {
            Route::Plugin {
                plugin,
                device,
                page,
            } => self.plugin_page(plugin, device, page),
            // The other pages the core owns arrive in later steps.
            _ => self.devices.view(&self.plugins),
        };
        if self.toasts.is_empty() {
            page
        } else {
            stack![page, toasts(&self.toasts)].into()
        }
    }

    fn plugin_page<'a>(&'a self, id: &str, device: &str, page: &str) -> Element<'a, Message> {
        self.plugins
            .iter()
            .find(|plugin| plugin.id() == id)
            .zip(self.devices.device(device))
            .and_then(|(plugin, device)| plugin.view_page(&self.ctx, device, page))
            .map_or_else(
                || {
                    container(text("This page no longer exists.").style(text::secondary))
                        .center(Length::Fill)
                        .into()
                },
                |page| page.map(Message::Plugin),
            )
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(
            [
                sync::watch(self.ctx.core()).map(Message::Sync),
                window::events().map(|(id, event)| Message::Window(id, event)),
            ]
            .into_iter()
            .chain(
                self.plugins
                    .iter()
                    .map(|plugin| plugin.subscription().map(Message::Plugin)),
            ),
        )
    }
}

/// A plugin's command, as the shell's task.
fn shell_task(command: Command<PluginMessage>) -> Task<Message> {
    command.into_task().map(|outcome| match outcome {
        Outcome::Plugin(message) => Message::Plugin(message),
        Outcome::Shell(request) => Message::Shell(request),
    })
}

/// The toasts, stacked at the bottom of the window, newest last.
fn toasts(toasts: &[Toast]) -> Element<'_, Message> {
    let toasts = toasts.iter().map(|toast| {
        let mut content = row![text(&toast.text).size(14).width(Length::Fill)]
            .spacing(12)
            .align_y(Alignment::Center);
        if let Some((label, route)) = &toast.action {
            content = content.push(
                button(text(label).size(14))
                    .style(button::text)
                    .on_press(Message::Shell(ShellRequest::Navigate(route.clone()))),
            );
        }
        container(content)
            .padding([10, 16])
            .max_width(400)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style {
                    background: Some(Background::Color(palette.background.strong.color)),
                    text_color: Some(palette.background.strong.text),
                    border: Border::default().rounded(8),
                    ..container::Style::default()
                }
            })
            .into()
    });
    container(column(toasts).spacing(8).align_x(Alignment::Center))
        .padding(16)
        .center_x(Length::Fill)
        .align_bottom(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use iced_fonts::lucide;

    use super::*;
    use crate::{
        core::{DeviceSnapshot, testing::handle},
        ui::plugin::{DeviceAction, UiPlugin},
    };

    /// Asks the shell for a toast and to open its page.
    struct Opener;

    #[derive(Debug, Clone)]
    enum OpenerMessage {
        Open(String),
    }

    impl UiPlugin for Opener {
        type Message = OpenerMessage;

        fn id(&self) -> &'static str {
            "opener"
        }

        fn device_actions(&self, device: &DeviceSnapshot) -> Vec<DeviceAction<OpenerMessage>> {
            vec![DeviceAction {
                id: "open",
                label: "Open".into(),
                icon: lucide::folder_open,
                enabled: true,
                visible_in_tray: true,
                message: OpenerMessage::Open(device.device_id.clone()),
            }]
        }

        fn update(&mut self, _ctx: &UiContext, message: OpenerMessage) -> Command<OpenerMessage> {
            let OpenerMessage::Open(device) = message;
            Command::batch([
                Command::shell(ShellRequest::toast("Opening")),
                Command::shell(ShellRequest::Navigate(Route::Plugin {
                    plugin: "opener",
                    device,
                    page: "files".into(),
                })),
            ])
        }
    }

    #[tokio::test]
    async fn a_plugin_can_toast_and_navigate() {
        let (core, _commands) = handle();
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        let (mut app, _) = App::boot(ctx, vec![Box::new(Opener)], false);

        let action = app.plugins[0]
            .device_actions(&testing::device("Phone"))
            .remove(0);
        let requests = testing::outputs(app.update(Message::Plugin(action.message))).await;
        assert_eq!(requests.len(), 2);
        for request in requests {
            // The toast's dismissal waits on a timer; don't run it.
            let _ = app.update(request);
        }

        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].text, "Opening");
        assert!(matches!(
            &app.route,
            Route::Plugin { plugin: "opener", page, .. } if page == "files"
        ));
        let _ = app.update(Message::DismissToast(app.toasts[0].id));
        assert!(app.toasts.is_empty());
    }

    /// iced runs `update` and polls tasks on threads with no tokio runtime.
    #[test]
    fn timers_work_off_the_daemon_runtime() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (core, _commands) = {
            let _entered = runtime.enter();
            handle()
        };
        let ctx = UiContext::new(core, runtime.handle().clone());
        let (app, _) = App::boot(ctx, Vec::new(), false);

        let task = app.after(Duration::from_millis(1), Message::DismissToast(7));
        let outputs = iced::futures::executor::block_on(testing::outputs(task));
        assert!(matches!(outputs[..], [Message::DismissToast(7)]));
    }

    #[test]
    fn snapshot_toasts() {
        let toasts = [
            Toast {
                id: 0,
                text: "Pinged Pixel 8a.".into(),
                action: None,
            },
            Toast {
                id: 1,
                text: "Downloading holiday.jpg".into(),
                action: Some(("Transfers".into(), Route::Transfers)),
            },
        ];
        testing::snapshot("toasts", (440.0, 300.0), || super::toasts(&toasts));
    }
}
