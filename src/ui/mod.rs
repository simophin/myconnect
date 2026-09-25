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
//!
//! [`Core`]: crate::core::Core
//! [`Core::subscribe`]: crate::core::Core::subscribe

pub mod demo;
pub mod error;
pub mod overlay;
pub mod pages;
pub mod plugin;
pub mod route;
pub mod sync;
#[cfg(test)]
pub(crate) mod testing;
pub mod widgets;

use std::{
    cell::RefCell,
    fmt,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use anyhow::{Context, Result};
use iced::{
    Element, Event, Size, Subscription, Task, event,
    keyboard::{self, key},
    widget::stack,
    window,
};
use iced_fonts::lucide;

use crate::daemon::RunningService;
use overlay::{
    dialog::{Dialog, DialogEvent, Dialogs, Field, Step, Submit},
    toast::{self, Toasts},
};
use pages::{devices::DeviceList, startup};
use plugin::{Command, ErasedUiPlugin, Outcome, PluginMessage, ShellRequest, UiContext};
use route::Route;

/// How the UI runs, besides the service it shows.
#[derive(Clone)]
pub struct UiOptions {
    /// The daemon's runtime. Work that touches the daemon's sockets runs
    /// here, not on iced's executor.
    pub runtime: tokio::runtime::Handle,
    /// Fill the core with made-up devices ([`demo`]).
    pub demo: bool,
}

/// A started daemon, and the UI halves of the plugins it runs.
pub struct Started {
    pub service: RunningService,
    /// Every feature's UI half, in `plugins::builtin_with_ui` order.
    pub plugins: Vec<Box<dyn ErasedUiPlugin>>,
}

/// Starting the daemon, which may fail.
pub type StartFuture = Pin<Box<dyn Future<Output = Result<Started>> + Send>>;

/// Show the UI until the window closes, starting the daemon with `start`
/// (again on Retry, if it fails), and shut the daemon down after.
pub fn run(options: UiOptions, start: impl Fn() -> StartFuture + 'static) -> Result<()> {
    let runtime = options.runtime.clone();
    let service: ServiceSlot = Arc::default();
    let boot = {
        let service = service.clone();
        let start: Rc<dyn Fn() -> StartFuture> = Rc::new(start);
        // iced asks for the state through a `Fn`; it boots once.
        let options = RefCell::new(Some(options));
        move || {
            let options = options.take().expect("the UI boots once");
            App::boot(options, start.clone(), service.clone())
        }
    };
    let result = iced::daemon(boot, App::update, App::view)
        .title("MyConnect")
        .subscription(App::subscription)
        .font(iced_fonts::LUCIDE_FONT_BYTES)
        .run()
        .context("UI failed");

    let service = service
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    if let Some(service) = service {
        runtime.block_on(service.shutdown())?;
    }
    result
}

/// The running daemon, shared with [`run`], which shuts it down after the
/// UI exits.
type ServiceSlot = Arc<Mutex<Option<RunningService>>>;

/// A value that isn't `Clone` passed through a message, which must be: the
/// first to [`take`](Handoff::take) it gets it.
struct Handoff<T>(Arc<Mutex<Option<T>>>);

impl<T> Clone for Handoff<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Handoff<T> {
    fn new(value: T) -> Self {
        Self(Arc::new(Mutex::new(Some(value))))
    }

    fn take(&self) -> Option<T> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

impl<T> fmt::Debug for Handoff<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Handoff(..)")
    }
}

#[derive(Debug, Clone)]
enum Message {
    /// The daemon started, or why it didn't.
    Started(Result<Handoff<Started>, String>),
    /// Start the daemon again after it failed.
    Retry,
    Sync(sync::Update),
    /// A message for the plugin it names.
    Plugin(PluginMessage),
    /// A plugin's request to the shell.
    Shell(ShellRequest<PluginMessage>),
    /// Go to the page this one was opened from.
    Back,
    /// A toast's button: go to its route.
    ToastAction(u64, Route),
    DismissToast(u64),
    Dialog(DialogEvent),
    /// A dialog's work finished; an error keeps it open.
    DialogFinished(u64, Result<(), String>),
    Key(KeyCommand),
    DemoTick(u64),
    WindowOpened,
    Window(window::Id, window::Event),
}

/// Keyboard shortcuts the shell handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyCommand {
    /// Escape: close the dialog.
    Cancel,
    /// Ctrl/Cmd+W.
    CloseWindow,
    /// Ctrl/Cmd+Q.
    Quit,
}

struct App {
    options: UiOptions,
    start: Rc<dyn Fn() -> StartFuture>,
    service: ServiceSlot,
    /// The main window.
    window: window::Id,
    window_focused: bool,
    phase: Phase,
    route: Route,
    toasts: Toasts,
    dialogs: Dialogs<Message>,
}

/// Whether the daemon runs yet.
enum Phase {
    Starting,
    /// It didn't start: why.
    Failed(String),
    Running(Box<Running>),
}

/// The UI once the daemon runs.
struct Running {
    ctx: UiContext,
    /// Every feature's UI half, in `plugins::builtin_with_ui` order.
    plugins: Vec<Box<dyn ErasedUiPlugin>>,
    devices: DeviceList,
}

impl App {
    fn boot(
        options: UiOptions,
        start: Rc<dyn Fn() -> StartFuture>,
        service: ServiceSlot,
    ) -> (Self, Task<Message>) {
        let (window, open) = window::open(window::Settings {
            size: Size::new(440.0, 620.0),
            ..window::Settings::default()
        });
        let mut app = Self {
            options,
            start,
            service,
            window,
            window_focused: true,
            phase: Phase::Starting,
            route: Route::Devices,
            toasts: Toasts::default(),
            dialogs: Dialogs::default(),
        };
        let start = app.start();
        (
            app,
            Task::batch([open.map(|_| Message::WindowOpened), start]),
        )
    }

    /// Start the daemon, off the UI thread so the window paints meanwhile.
    fn start(&mut self) -> Task<Message> {
        self.phase = Phase::Starting;
        Task::future(self.options.runtime.spawn((self.start)())).map(|joined| {
            Message::Started(match joined {
                Ok(Ok(started)) => Ok(Handoff::new(started)),
                Ok(Err(error)) => Err(format!("{error:#}")),
                Err(error) => Err(error.to_string()),
            })
        })
    }

    fn started(&mut self, started: Started) -> Task<Message> {
        let core = started.service.core().clone();
        *self.service.lock().unwrap_or_else(PoisonError::into_inner) = Some(started.service);
        let ids: Vec<_> = started.plugins.iter().map(|plugin| plugin.id()).collect();
        tracing::debug!(plugins = ?ids, "UI plugins");
        let mut ctx = UiContext::new(core, self.options.runtime.clone());
        ctx.set_window_focused(self.window_focused);
        self.phase = Phase::Running(Box::new(Running {
            devices: DeviceList::new(ctx.core().local_device_name()),
            ctx,
            plugins: started.plugins,
        }));
        if self.options.demo
            && let Phase::Running(running) = &self.phase
        {
            demo::start(running.ctx.core());
            return Task::done(Message::DemoTick(0));
        }
        Task::none()
    }

    fn running(&mut self) -> Option<&mut Running> {
        match &mut self.phase {
            Phase::Running(running) => Some(running),
            _ => None,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Started(Ok(started)) => match started.take() {
                Some(started) => self.started(started),
                None => Task::none(),
            },
            Message::Started(Err(error)) => {
                tracing::error!(%error, "the daemon did not start");
                self.phase = Phase::Failed(error);
                Task::none()
            }
            Message::Retry => match self.phase {
                Phase::Failed(_) => self.start(),
                _ => Task::none(),
            },
            Message::Sync(update) => {
                let Some(running) = self.running() else {
                    return Task::none();
                };
                let mut tasks = Vec::new();
                if let sync::Update::Event(event) = &update {
                    for plugin in &mut running.plugins {
                        tasks.push(shell_task(plugin.on_event(&running.ctx, event)));
                    }
                }
                running.devices.update(update);
                Task::batch(tasks)
            }
            Message::Plugin(message) => {
                let Some(running) = self.running() else {
                    return Task::none();
                };
                let Some(plugin) = running
                    .plugins
                    .iter_mut()
                    .find(|plugin| plugin.id() == message.plugin())
                else {
                    tracing::warn!(?message, "message for an unknown plugin");
                    return Task::none();
                };
                shell_task(plugin.update(&running.ctx, message))
            }
            Message::Shell(request) => self.handle(request),
            Message::Back => {
                if let Some(parent) = self.route.parent() {
                    self.route = parent;
                }
                Task::none()
            }
            Message::ToastAction(id, route) => {
                self.toasts.dismiss(id);
                self.route = route;
                Task::none()
            }
            Message::DismissToast(id) => {
                self.toasts.dismiss(id);
                Task::none()
            }
            Message::Dialog(event) => self.dialog(event),
            Message::DialogFinished(id, result) => {
                let closed = result.is_ok();
                self.dialogs.finished(id, result);
                if closed {
                    self.dialogs.focus()
                } else {
                    Task::none()
                }
            }
            Message::Key(KeyCommand::Cancel) => self.dialog(DialogEvent::Cancel),
            Message::Key(KeyCommand::CloseWindow) => window::close(self.window),
            Message::Key(KeyCommand::Quit) => iced::exit(),
            Message::DemoTick(tick) => {
                let Some(running) = self.running() else {
                    return Task::none();
                };
                demo::tick(running.ctx.core(), &running.plugins, tick);
                self.after(demo::TICK, Message::DemoTick(tick + 1))
            }
            Message::WindowOpened => Task::none(),
            Message::Window(id, event) if id == self.window => match event {
                window::Event::Focused | window::Event::Unfocused => {
                    self.window_focused = event == window::Event::Focused;
                    let focused = self.window_focused;
                    if let Some(running) = self.running() {
                        running.ctx.set_window_focused(focused);
                    }
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

    fn dialog(&mut self, event: DialogEvent) -> Task<Message> {
        let closes = matches!(event, DialogEvent::Cancel);
        match self.dialogs.update(event) {
            Step::Nothing if closes => self.dialogs.focus(),
            Step::Nothing => Task::none(),
            Step::Send(message) => Task::batch([self.dialogs.focus(), Task::done(message)]),
            Step::Run(id, work) => work.map(move |result| Message::DialogFinished(id, result)),
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
            ShellRequest::Confirm {
                title,
                body,
                confirm_label,
                then,
            } => self.dialogs.open(Dialog::confirm(
                title,
                body,
                confirm_label,
                Submit::Close(Arc::new(move |_| Message::Plugin(then.clone()))),
            )),
            ShellRequest::Prompt {
                title,
                label,
                initial,
                confirm_label,
                validate,
                then,
            } => self.dialogs.open(Dialog::prompt(
                title,
                Field {
                    value: initial,
                    label: Some(label),
                    validate: Some(validate),
                    ..Field::default()
                },
                confirm_label,
                Submit::Close(Arc::new(move |text| Message::Plugin(then(text)))),
            )),
            request @ ShellRequest::PickFiles { .. } => {
                tracing::warn!(?request, "the shell can't do this yet");
                Task::none()
            }
        }
    }

    fn toast(&mut self, text: String, action: Option<(String, Route)>) -> Task<Message> {
        let id = self.toasts.push(text, action);
        self.after(toast::DURATION, Message::DismissToast(id))
    }

    /// Send `message` after `delay`, timed on the daemon's runtime: iced's
    /// executor has no timer.
    fn after(&self, delay: Duration, message: Message) -> Task<Message> {
        // `sleep` needs the runtime when it is made, not only when polled.
        plugin::on_runtime(&self.options.runtime, async move {
            tokio::time::sleep(delay).await;
        })
        .map(move |()| message.clone())
    }

    fn view(&self, _window: window::Id) -> Element<'_, Message> {
        let page = match &self.phase {
            Phase::Starting => startup::starting(),
            Phase::Failed(error) => startup::failed(error, Message::Retry),
            Phase::Running(running) => self.page(running),
        };
        let page = if self.toasts.is_empty() {
            page
        } else {
            stack![page, self.toasts.view(Message::ToastAction)].into()
        };
        self.dialogs.view(page, Message::Dialog)
    }

    /// The page for the current route.
    fn page<'a>(&'a self, running: &'a Running) -> Element<'a, Message> {
        let title = match &self.route {
            Route::Devices => return running.devices.view(&running.plugins),
            Route::Plugin {
                plugin,
                device,
                page,
            } => return self.plugin_page(running, plugin, device, page),
            Route::Device(id) => running
                .devices
                .device(id)
                .map_or("Device", |device| device.device_name.as_str()),
            Route::AddDevice => "Add device",
            Route::Pairing(_) => "Pairing",
            Route::Transfers => "Transfers",
            Route::Settings => "Settings",
        };
        // The other pages the core owns arrive in later steps.
        widgets::page(
            widgets::page_header(title, Some(Message::Back), vec![]),
            widgets::empty_state(lucide::construction, "Not here yet", None),
        )
    }

    fn plugin_page<'a>(
        &'a self,
        running: &'a Running,
        id: &str,
        device: &str,
        page: &str,
    ) -> Element<'a, Message> {
        running
            .plugins
            .iter()
            .find(|plugin| plugin.id() == id)
            .zip(running.devices.device(device))
            .and_then(|(plugin, device)| plugin.view_page(&running.ctx, device, page))
            .map_or_else(
                || {
                    widgets::page(
                        widgets::page_header("", Some(Message::Back), vec![]),
                        widgets::empty_state(
                            lucide::circle_alert,
                            "This page no longer exists.",
                            None,
                        ),
                    )
                },
                |page| page.map(Message::Plugin),
            )
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            window::events().map(|(id, event)| Message::Window(id, event)),
            event::listen_with(key_command).map(Message::Key),
        ];
        if let Phase::Running(running) = &self.phase {
            subscriptions.push(sync::watch(running.ctx.core()).map(Message::Sync));
            subscriptions.extend(
                running
                    .plugins
                    .iter()
                    .map(|plugin| plugin.subscription().map(Message::Plugin)),
            );
        }
        Subscription::batch(subscriptions)
    }
}

/// The shortcut a key press means, if any. Escape counts even when a text
/// field took it, so it closes a dialog from its field.
fn key_command(event: Event, _status: event::Status, _window: window::Id) -> Option<KeyCommand> {
    let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event else {
        return None;
    };
    match key.as_ref() {
        keyboard::Key::Named(key::Named::Escape) => Some(KeyCommand::Cancel),
        keyboard::Key::Character("w") if modifiers.command() => Some(KeyCommand::CloseWindow),
        keyboard::Key::Character("q") if modifiers.command() => Some(KeyCommand::Quit),
        _ => None,
    }
}

/// A plugin's command, as the shell's task.
fn shell_task(command: Command<PluginMessage>) -> Task<Message> {
    command.into_task().map(|outcome| match outcome {
        Outcome::Plugin(message) => Message::Plugin(message),
        Outcome::Shell(request) => Message::Shell(request),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use iced_fonts::lucide;

    use super::*;
    use crate::{
        core::{DeviceSnapshot, testing::handle},
        ui::plugin::{DeviceAction, UiPlugin},
    };

    /// An app whose daemon never starts: tests set the phase themselves.
    fn app(runtime: tokio::runtime::Handle) -> App {
        let start: Rc<dyn Fn() -> StartFuture> = Rc::new(|| Box::pin(std::future::pending()));
        App::boot(
            UiOptions {
                runtime,
                demo: false,
            },
            start,
            ServiceSlot::default(),
        )
        .0
    }

    /// An app running `plugins` over a test core.
    fn running(plugins: Vec<Box<dyn ErasedUiPlugin>>) -> App {
        let runtime = tokio::runtime::Handle::current();
        let (core, _commands) = handle();
        let mut app = app(runtime.clone());
        app.phase = Phase::Running(Box::new(Running {
            devices: DeviceList::new(core.local_device_name()),
            ctx: UiContext::new(core, runtime),
            plugins,
        }));
        app
    }

    /// Asks the shell for a toast and to open its page, and to confirm.
    struct Opener {
        confirmed: Arc<AtomicUsize>,
    }

    #[derive(Debug, Clone)]
    enum OpenerMessage {
        Open(String),
        Delete,
        Deleted,
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
            match message {
                OpenerMessage::Open(device) => Command::batch([
                    Command::shell(ShellRequest::toast("Opening")),
                    Command::shell(ShellRequest::Navigate(Route::Plugin {
                        plugin: "opener",
                        device,
                        page: "files".into(),
                    })),
                ]),
                OpenerMessage::Delete => Command::shell(ShellRequest::Confirm {
                    title: "Delete it?".into(),
                    body: "This can’t be undone.".into(),
                    confirm_label: "Delete".into(),
                    then: OpenerMessage::Deleted,
                }),
                OpenerMessage::Deleted => {
                    self.confirmed.fetch_add(1, Ordering::SeqCst);
                    Command::none()
                }
            }
        }
    }

    fn opener() -> (Box<dyn ErasedUiPlugin>, Arc<AtomicUsize>) {
        let confirmed = Arc::new(AtomicUsize::new(0));
        (
            Box::new(Opener {
                confirmed: confirmed.clone(),
            }),
            confirmed,
        )
    }

    /// Run `message` through `app`, then every message it leads to, except
    /// a toast's dismissal. Tests that toast run with time paused, so the
    /// dismissal's timer doesn't hold them up.
    async fn settle(app: &mut App, message: Message) {
        let mut queue = std::collections::VecDeque::from([message]);
        while let Some(message) = queue.pop_front() {
            if !matches!(message, Message::DismissToast(_)) {
                queue.extend(testing::outputs(app.update(message)).await);
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_plugin_can_toast_and_navigate() {
        let (plugin, _) = opener();
        let mut app = running(vec![plugin]);
        let Phase::Running(running) = &app.phase else {
            unreachable!()
        };
        let action = running.plugins[0]
            .device_actions(&testing::device("Phone"))
            .remove(0);

        settle(&mut app, Message::Plugin(action.message)).await;

        assert_eq!(app.toasts.items().len(), 1);
        assert_eq!(app.toasts.items()[0].text, "Opening");
        assert!(matches!(
            &app.route,
            Route::Plugin { plugin: "opener", page, .. } if page == "files"
        ));
        let _ = app.update(Message::DismissToast(app.toasts.items()[0].id));
        assert!(app.toasts.is_empty());
    }

    #[tokio::test]
    async fn back_goes_to_the_parent_page() {
        let mut app = running(Vec::new());
        app.route = Route::Plugin {
            plugin: "opener",
            device: "phone".into(),
            page: "files".into(),
        };
        let _ = app.update(Message::Back);
        assert_eq!(app.route, Route::Device("phone".into()));
        let _ = app.update(Message::Back);
        let _ = app.update(Message::Back);
        assert_eq!(app.route, Route::Devices);
    }

    #[tokio::test]
    async fn a_toast_button_goes_to_its_page_and_dismisses_it() {
        let mut app = running(Vec::new());
        let _ = app.toast(
            "Downloading holiday.jpg".into(),
            Some(("Transfers".into(), Route::Transfers)),
        );
        let id = app.toasts.items()[0].id;
        let _ = app.update(Message::ToastAction(id, Route::Transfers));
        assert_eq!(app.route, Route::Transfers);
        assert!(app.toasts.is_empty());
    }

    #[tokio::test]
    async fn a_plugin_confirm_sends_its_message_only_when_confirmed() {
        let (plugin, confirmed) = opener();
        let mut app = running(vec![plugin]);
        let delete = || Message::Plugin(PluginMessage::new("opener", OpenerMessage::Delete));

        settle(&mut app, delete()).await;
        assert_eq!(app.dialogs.current().unwrap().title, "Delete it?");
        settle(&mut app, Message::Key(KeyCommand::Cancel)).await;
        assert!(app.dialogs.current().is_none());
        assert_eq!(confirmed.load(Ordering::SeqCst), 0);

        settle(&mut app, delete()).await;
        settle(&mut app, Message::Dialog(DialogEvent::Submit)).await;
        assert!(app.dialogs.current().is_none());
        assert_eq!(confirmed.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_failed_start_shows_why_and_retry_starts_again() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let start: Rc<dyn Fn() -> StartFuture> = {
            let attempts = attempts.clone();
            Rc::new(move || {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                Box::pin(async move { anyhow::bail!("attempt {attempt} failed") })
            })
        };
        let (mut app, _) = App::boot(
            UiOptions {
                runtime: runtime.handle().clone(),
                demo: false,
            },
            start,
            ServiceSlot::default(),
        );
        assert!(matches!(app.phase, Phase::Starting));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);

        // Booting started it once; Retry does nothing until it fails.
        let task = app.update(Message::Retry);
        assert!(iced::futures::executor::block_on(testing::outputs(task)).is_empty());
        let _ = app.update(Message::Started(Err("attempt 1 failed".into())));
        assert!(matches!(&app.phase, Phase::Failed(error) if error == "attempt 1 failed"));

        let task = app.update(Message::Retry);
        assert!(matches!(app.phase, Phase::Starting));
        let outputs = iced::futures::executor::block_on(testing::outputs(task));
        let [Message::Started(Err(error))] = &outputs[..] else {
            panic!("unexpected outputs: {outputs:?}");
        };
        assert_eq!(error, "attempt 2 failed");
    }

    #[test]
    fn keys_map_to_commands() {
        let press = |key: keyboard::Key, modifiers| {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                modifiers,
                text: None,
                repeat: false,
            })
        };
        let window = window::Id::unique();
        let command = |event| key_command(event, event::Status::Captured, window);
        assert_eq!(
            command(press(
                keyboard::Key::Named(key::Named::Escape),
                keyboard::Modifiers::empty()
            )),
            Some(KeyCommand::Cancel)
        );
        assert_eq!(
            command(press(
                keyboard::Key::Character("w".into()),
                keyboard::Modifiers::COMMAND
            )),
            Some(KeyCommand::CloseWindow)
        );
        assert_eq!(
            command(press(
                keyboard::Key::Character("q".into()),
                keyboard::Modifiers::empty()
            )),
            None
        );
    }

    /// iced runs `update` and polls tasks on threads with no tokio runtime.
    #[test]
    fn timers_work_off_the_daemon_runtime() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let app = app(runtime.handle().clone());

        let task = app.after(Duration::from_millis(1), Message::DismissToast(7));
        let outputs = iced::futures::executor::block_on(testing::outputs(task));
        assert!(matches!(outputs[..], [Message::DismissToast(7)]));
    }
}
