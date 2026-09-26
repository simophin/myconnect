//! The desktop UI, in iced (`gui` feature).
//!
//! The UI runs in the daemon's process and talks to the core directly:
//! snapshots from [`Core`], events from [`Core::subscribe`], and actions
//! through typed Rust functions, never the HTTP API. The daemon still serves
//! that API, so the CLI can drive and inspect the instance the UI shows.
//!
//! This module is the shell: the window, the tray, the pages it owns, and
//! what features share. Each feature's UI lives in [`features`], which the
//! shell calls by name. See `docs/adr/0001-native-ui-in-iced.md`.
//!
//! [`Core`]: crate::core::Core
//! [`Core::subscribe`]: crate::core::Core::subscribe

pub mod activity;
pub mod background;
pub mod context;
pub mod demo;
pub mod desktop;
pub mod error;
pub mod features;
pub mod overlay;
pub mod pages;
pub mod route;
pub mod shell;
pub mod store;
pub mod sync;
#[cfg(test)]
pub(crate) mod testing;
pub mod widgets;

use std::{
    cell::RefCell,
    fmt,
    future::Future,
    net::Ipv4Addr,
    path::PathBuf,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use anyhow::{Context, Result};
use iced::{
    Element, Event, Subscription, Task, event,
    keyboard::{self, key},
    widget::stack,
    window,
};
use iced_fonts::lucide;
use uuid::Uuid;

use crate::{
    config,
    core::{Core, CoreError, PairingSnapshot, SettingsPatch, SettingsSnapshot},
    daemon::RunningService,
    plugins::{browse::BrowsePlugin, clipboard::ClipboardPlugin},
};
use context::UiContext;
use desktop::{
    DesktopEvent,
    dialogs::{self as picking, Pick},
    instance::{self, Instance},
    notify::{self as notifying, Notifier},
    open::{self as opening, Open},
    placement::{Placement, PlacementStore, Seen},
    tray::{self as trays, Tray, TrayCommand, TrayItem},
    window::{self as windowing, Windows},
};
use features::{DropTarget, Feature, Features};
use overlay::{
    dialog::{self as dialogs, Dialog, DialogEvent, Dialogs, Field, Step, Submit},
    drop::{self as dropping, Drag, Dropped},
    incoming,
    toast::{self, Toasts},
};
use pages::{add_device, device, devices, pairing, settings, startup, transfers};
use route::Route;
use store::Snapshot;

/// How the UI runs, besides the service it shows.
#[derive(Clone)]
pub struct UiOptions {
    /// The daemon's runtime. Work that touches the daemon's sockets runs
    /// here, not on iced's executor.
    pub runtime: tokio::runtime::Handle,
    /// Fill the core with made-up devices ([`demo`]).
    pub demo: bool,
    /// The app's version, for Settings.
    pub version: String,
    /// The data directory given (`--data-dir`), if any: `window.json` goes
    /// there, and it names the single-instance socket.
    pub data_dir: Option<PathBuf>,
}

/// A started daemon, and the plugins it runs that the UI calls too
/// (`plugins::builtin_parts`).
pub struct Started {
    pub service: RunningService,
    pub clipboard: Arc<ClipboardPlugin>,
    pub browse: Arc<BrowsePlugin>,
}

/// Starting the daemon, which may fail.
pub type StartFuture = Pin<Box<dyn Future<Output = Result<Started>> + Send>>;

/// Show the UI until the user quits, starting the daemon with `start`
/// (again on Retry, if it fails), and shut the daemon down after. With the
/// window closed, the app keeps running in the tray. If another launch
/// already runs with the same data directory, show its window instead.
pub fn run(options: UiOptions, start: impl Fn() -> StartFuture + 'static) -> Result<()> {
    let runtime = options.runtime.clone();
    let (events, received) = tokio::sync::mpsc::unbounded_channel();
    let data_dir = options.data_dir.clone().or_else(config::default_config_dir);
    if let Some(data_dir) = &data_dir
        && let Instance::Running = instance::claim(data_dir, events.clone())
    {
        tracing::info!("MyConnect is already running; showing its window");
        return Ok(());
    }
    // Before the daemon starts, so the tray works even if it doesn't.
    let (tray, tray_available) = trays::spawn(&runtime, events.clone());
    let desktop = Desktop {
        tray,
        tray_available,
        notifier: notifying::start(&runtime, events.clone()),
        windows: Arc::new(windowing::System),
        placements: Some(PlacementStore::for_data_dir(options.data_dir.as_deref())),
        events: Some(desktop::Receiver::new(received)),
        opener: Arc::new(opening::System),
        picker: Arc::new(picking::System),
    };
    desktop::watch_quit_signals(&runtime, events);

    let (program, service) = program(options, start, desktop);
    let result = program.run().context("UI failed");
    if let Some(service) = service.take() {
        runtime.block_on(service.shutdown())?;
    }
    result
}

/// The UI as an iced program on `desktop`, and the daemon it starts, kept
/// for the caller to shut down once the program ends. [`run`] runs it in
/// real windows; the end-to-end tests run it in `iced_test`'s emulator.
pub fn program(
    options: UiOptions,
    start: impl Fn() -> StartFuture + 'static,
    desktop: Desktop,
) -> (iced::Daemon<impl iced::Program>, Service) {
    let service = Service::default();
    let boot = {
        let service = service.clone();
        let start: Rc<dyn Fn() -> StartFuture> = Rc::new(start);
        // iced asks for the state through a `Fn`; it boots once.
        let booting = RefCell::new(Some((options, desktop)));
        move || {
            let (options, desktop) = booting.take().expect("the UI boots once");
            App::boot(options, start.clone(), service.clone(), desktop)
        }
    };
    let program = iced::daemon(boot, App::update, App::view)
        .title("MyConnect")
        .subscription(App::subscription)
        .font(iced_fonts::LUCIDE_FONT_BYTES);
    (program, service)
}

/// The daemon the UI started, once it has.
#[derive(Clone, Default)]
pub struct Service(Arc<Mutex<Option<RunningService>>>);

impl Service {
    fn set(&self, service: RunningService) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(service);
    }

    /// The daemon, to shut it down; `None` if it never started.
    pub fn take(&self) -> Option<RunningService> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

/// The desktop around the window: the tray, notifications, the window
/// itself and where it was, pickers and file openers. [`run`] uses the
/// real ones; tests pass fakes to [`program`].
pub struct Desktop {
    pub tray: Arc<dyn Tray>,
    /// A tray host shows the icon now. Without one, a closed window could
    /// come back only by launching the app again, so closing quits, and
    /// the window opens at start.
    pub tray_available: bool,
    pub notifier: Arc<dyn Notifier>,
    pub windows: Arc<dyn Windows>,
    /// Where the placement is kept; `None` keeps it in memory only.
    pub placements: Option<PlacementStore>,
    /// The desktop's events, for the subscription.
    pub events: Option<desktop::Receiver>,
    /// Opens received files.
    pub opener: Arc<dyn Open>,
    /// The desktop's file and folder pickers.
    pub picker: Arc<dyn Pick>,
}

/// Where a feature's message came from, which decides how its outcome is
/// shown: in the window, or, from the tray, without showing the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    Window,
    Tray,
}

/// How long the window must stay put before its placement is saved: moves
/// and resizes arrive continuously while dragging.
const PLACEMENT_SAVE_DELAY: Duration = Duration::from_millis(500);

/// A value that isn't `Clone` passed through a message, which must be: the
/// first to [`take`](Handoff::take) it gets it.
pub(crate) struct Handoff<T>(Arc<Mutex<Option<T>>>);

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
pub(crate) enum Message {
    /// The daemon started, or why it didn't.
    Started(Result<Handoff<Started>, String>),
    /// Start the daemon again after it failed.
    Retry,
    Sync(sync::Update),
    /// Read the core again after a snapshot failed. Events already queued
    /// are applied after it, as after the subscription's own snapshot.
    Reload,
    /// A feature's message, and where the action that caused it came from.
    /// Its follow-up messages keep that origin.
    Feature(Feature, Origin),
    /// A short message at the bottom of the window, with an optional button
    /// that goes somewhere. Not shown for an action from the tray.
    Toast {
        text: String,
        action: Option<(String, Route)>,
        origin: Origin,
    },
    /// How an action went. The window shows `text` as a toast. An action
    /// chosen in the tray, with the window likely closed, reports only a
    /// failure: a notification titled `failure` ("Couldn’t ping Pixel")
    /// over `text`.
    Report {
        text: String,
        failure: Option<String>,
        origin: Origin,
    },
    /// A toast while the window is focused, a desktop notification
    /// otherwise.
    Notify {
        title: String,
        body: String,
    },
    /// Ask before doing something; `then` is sent on confirm. From the
    /// tray, the window shows too.
    Confirm {
        title: String,
        body: String,
        confirm_label: String,
        then: Feature,
        origin: Origin,
    },
    /// Ask for a line of text. From the tray, the window shows too.
    Prompt(Box<shell::Prompt>),
    /// Pick files. From the tray, the files go back as the tray's, so a
    /// failure to send them is reported the tray's way.
    PickFiles(shell::PickFiles),
    /// Something the desktop reported: the tray, a notification, a second
    /// launch.
    Desktop(DesktopEvent),
    /// Close the window to the tray, now that it was seen where it is.
    Hide(Seen),
    /// Quit, saving the window as last seen, if it showed.
    Exit(Option<Seen>),
    /// The window has stayed put since this move or resize: read its
    /// placement.
    SavePlacement(u64),
    /// The window's placement, read to save it.
    PlacementSeen(Seen),
    /// Go to a page; from the tray, show the window on it.
    Navigate(Route, Origin),
    /// Go to the page this one was opened from.
    Back,
    /// Ask whether to unpair a device.
    Unpair {
        device_id: String,
        name: String,
    },
    /// Unpair a device: confirmed.
    Forget {
        device_id: String,
    },
    /// Unpairing finished, or why it didn't.
    Forgotten {
        device_id: String,
        result: Result<(), String>,
    },
    /// A toast's button: go to its route.
    ToastAction(u64, Route),
    DismissToast(u64),
    Dialog(DialogEvent),
    /// A dialog's work finished with the message to send; an error keeps
    /// it open.
    DialogFinished(u64, Result<Box<Message>, String>),
    /// Announce this computer so devices nearby answer.
    Scan,
    /// Show the "searching" bar for a while.
    ShowSearching,
    /// The "searching" bar of this scan has shown long enough.
    SearchEnded(u64),
    /// Ask for an address to announce this computer to.
    AddByAddress,
    /// Start pairing with a device.
    Pair(String),
    /// Starting a pairing finished, or why it didn't.
    PairStarted(Result<PairingSnapshot, String>),
    /// Cancel an outgoing pairing.
    CancelPairing(Uuid),
    PairingCancelled(Uuid, Result<PairingSnapshot, String>),
    /// Accept or reject an incoming pairing request.
    AnswerPairing {
        pairing_id: Uuid,
        accept: bool,
    },
    PairingAnswered(Uuid, Result<PairingSnapshot, String>),
    /// Stop a transfer.
    CancelTransfer(Uuid),
    /// Open a received file with its default app.
    OpenFile(PathBuf),
    /// Show a received file in the file manager.
    RevealFile(PathBuf),
    /// Opening a file failed: which one, and why.
    OpenFailed(PathBuf, String),
    /// Ask for a new name for this computer.
    Rename,
    /// Pick the folder received files are saved in.
    ChooseDownloadDir,
    /// The folder picked, or `None` if the picker was cancelled.
    DownloadDirPicked(Option<PathBuf>),
    SetCloseToTray(bool),
    /// A settings change finished: the settings now, or why it failed.
    SettingsSaved(Result<SettingsSnapshot, String>),
    /// The wait for the rest of this drag's dropped files is over.
    DropSettled(u64),
    /// Send the dropped files to this device, chosen in the chooser.
    DropOn(String),
    /// Close the chooser without sending.
    CancelDrop,
    Key(KeyCommand),
    DemoTick(u64),
    WindowOpened,
    Window(window::Id, window::Event),
}

/// Keyboard shortcuts the shell handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyCommand {
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
    service: Service,
    desktop: Desktop,
    /// The main window, while it is open; `None` in the tray.
    window: Option<window::Id>,
    window_focused: bool,
    /// Where the window is, or was when it closed.
    placement: Placement,
    /// Which move or resize the pending placement save is for.
    placement_moves: u64,
    /// Quitting has started.
    quitting: bool,
    /// The tray menu as last sent.
    tray_menu: Vec<TrayItem>,
    notifications: background::Notifications,
    phase: Phase,
    route: Route,
    toasts: Toasts,
    dialogs: Dialogs<Message>,
    /// The device being unpaired, if any: its Unpair button is disabled.
    unpairing: Option<String>,
    /// Add device's "searching" bar shows.
    searching: bool,
    /// Which scan the bar is for, so an older scan's timer doesn't hide it.
    scan: u64,
    /// The device a pairing is being started with, if any.
    starting: Option<String>,
    /// The outgoing pairing being cancelled, if any.
    cancelling: Option<Uuid>,
    /// The incoming request being answered, if any.
    answering: Option<Uuid>,
    /// Why answering a request failed, shown in its prompt.
    answer_error: Option<(Uuid, String)>,
    /// Files being dragged over the window.
    drag: Drag,
    /// Dropped files waiting for the user to choose a device.
    choosing: Option<Vec<PathBuf>>,
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
    features: Features,
}

impl App {
    fn boot(
        options: UiOptions,
        start: Rc<dyn Fn() -> StartFuture>,
        service: Service,
        desktop: Desktop,
    ) -> (Self, Task<Message>) {
        let placement = desktop
            .placements
            .as_ref()
            .and_then(PlacementStore::load)
            .unwrap_or_default();
        let mut app = Self {
            options,
            start,
            service,
            desktop,
            window: None,
            window_focused: false,
            placement,
            placement_moves: 0,
            quitting: false,
            tray_menu: Vec::new(),
            notifications: background::Notifications::default(),
            phase: Phase::Starting,
            route: Route::Devices,
            toasts: Toasts::default(),
            dialogs: Dialogs::default(),
            unpairing: None,
            searching: false,
            scan: 0,
            starting: None,
            cancelling: None,
            answering: None,
            answer_error: None,
            drag: Drag::default(),
            choosing: None,
        };
        // Hidden if it was quit from the tray, unless there is no tray to
        // bring it back.
        let open = if app.placement.visible || !app.desktop.tray_available {
            app.open_window()
        } else {
            Task::none()
        };
        let start = app.start();
        app.update_tray();
        (app, Task::batch([open, start]))
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
        self.service.set(started.service);
        let mut ctx = UiContext::new(core, self.options.runtime.clone());
        ctx.set_window_focused(self.focused());
        self.phase = Phase::Running(Box::new(Running {
            ctx,
            features: Features::new(started.clipboard, started.browse),
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
        let refreshes = matches!(message, Message::Sync(_) | Message::Reload);
        let before = match &self.phase {
            Phase::Running(running) if refreshes => {
                Some(background::transfer_statuses(running.ctx.store()))
            }
            _ => None,
        };
        let task = self.react(message);
        if let Some(before) = before
            && let Phase::Running(running) = &self.phase
        {
            let received = background::received_files(&before, running.ctx.store());
            self.notify_received(received);
        }
        self.update_tray();
        self.notify_pairings();
        task
    }

    fn react(&mut self, message: Message) -> Task<Message> {
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
                match update {
                    sync::Update::Snapshot(snapshot) => {
                        running.ctx.store_mut().apply_snapshot(*snapshot);
                        Task::none()
                    }
                    // The store first, so features see the event applied.
                    sync::Update::Event(event) => {
                        running.ctx.store_mut().apply_event(&event.event);
                        running.features.on_event(&running.ctx, &event)
                    }
                }
            }
            Message::Reload => {
                if let Some(running) = self.running() {
                    let snapshot = Snapshot::take(running.ctx.core());
                    running.ctx.store_mut().apply_snapshot(snapshot);
                }
                Task::none()
            }
            Message::Feature(feature, origin) => self.feature(feature, origin),
            // From the tray, the window stays as it is unless the request
            // needs it (a page, a dialog), and only failures are reported.
            Message::Toast {
                origin: Origin::Tray,
                ..
            } => Task::none(),
            Message::Toast { text, action, .. } => self.toast(text, action),
            Message::Report {
                text,
                failure,
                origin: Origin::Tray,
            } => match failure {
                Some(title) => self.notify(&title, &text),
                None => Task::none(),
            },
            Message::Report { text, .. } => self.toast(text, None),
            Message::Notify { title, body } => self.notify(&title, &body),
            Message::Confirm {
                title,
                body,
                confirm_label,
                then,
                origin,
            } => {
                let show = self.show_window_for(origin);
                let open = self.dialogs.open(Dialog::confirm(
                    title,
                    body,
                    confirm_label,
                    Submit::Close(Arc::new(move |_| {
                        Message::Feature(then.clone(), Origin::Window)
                    })),
                ));
                Task::batch([show, open])
            }
            Message::Prompt(prompt) => self.prompt(*prompt),
            Message::PickFiles(pick) => self.pick_files(pick),
            Message::Desktop(event) => self.desktop_event(event),
            Message::Hide(seen) => self.hide(seen),
            Message::Exit(seen) => self.exit(seen),
            Message::SavePlacement(moves) => match self.window {
                Some(id) if moves == self.placement_moves => {
                    self.desktop.windows.read(id).map(Message::PlacementSeen)
                }
                _ => Task::none(),
            },
            Message::PlacementSeen(seen) => {
                if self.window.is_some() && !self.quitting {
                    let placement = self.placement.seen(seen, true);
                    if placement != self.placement {
                        self.placement = placement;
                        self.save_placement();
                    }
                }
                Task::none()
            }
            Message::Navigate(route, Origin::Window) => self.go(route),
            Message::Navigate(route, Origin::Tray) => {
                Task::batch([self.go(route), self.show_window()])
            }
            Message::Back => match self.route.parent() {
                Some(parent) => self.go(parent),
                None => Task::none(),
            },
            Message::Unpair { device_id, name } => self.dialogs.open(
                Dialog::confirm(
                    format!("Unpair {name}?"),
                    "The device will need to be paired again before it can exchange \
                     anything with this computer.",
                    "Unpair",
                    Submit::Close(Arc::new(move |_| Message::Forget {
                        device_id: device_id.clone(),
                    })),
                )
                .danger(),
            ),
            Message::Forget { device_id } => self.forget(device_id),
            Message::Forgotten { device_id, result } => {
                if self.unpairing.as_deref() == Some(device_id.as_str()) {
                    self.unpairing = None;
                }
                match result {
                    Ok(()) => {
                        if let Some(running) = self.running() {
                            running.ctx.store_mut().remove_device(&device_id);
                        }
                        // Leave the device's pages; elsewhere, stay put.
                        if self.route.device() == Some(device_id.as_str()) {
                            return self.go(Route::Devices);
                        }
                        Task::none()
                    }
                    Err(error) => self.toast(error, None),
                }
            }
            Message::ToastAction(id, route) => {
                self.toasts.dismiss(id);
                self.go(route)
            }
            Message::DismissToast(id) => {
                self.toasts.dismiss(id);
                Task::none()
            }
            Message::Dialog(event) => self.dialog(event),
            Message::DialogFinished(id, result) => {
                let closed = result.is_ok();
                let then = self.dialogs.finished(id, result.map(|message| *message));
                if closed {
                    Task::batch([
                        self.dialogs.focus(),
                        then.map_or_else(Task::none, Task::done),
                    ])
                } else {
                    Task::none()
                }
            }
            Message::Scan => self.scan(),
            Message::ShowSearching => self.show_searching(),
            Message::SearchEnded(scan) => {
                if scan == self.scan {
                    self.searching = false;
                }
                Task::none()
            }
            Message::AddByAddress => self.add_by_address(),
            Message::Pair(device_id) => self.pair(device_id),
            Message::PairStarted(result) => {
                self.starting = None;
                match result {
                    Ok(pairing) => {
                        let Some(running) = self.running() else {
                            return Task::none();
                        };
                        let pairing = running.ctx.store_mut().apply_pairing(pairing);
                        // Only if the user is still where they asked.
                        if matches!(self.route, Route::AddDevice | Route::Pairing(_)) {
                            return self.go(Route::Pairing(pairing.id));
                        }
                        Task::none()
                    }
                    Err(error) => self.toast(error, None),
                }
            }
            Message::CancelPairing(pairing_id) => self.cancel_pairing(pairing_id),
            Message::PairingCancelled(pairing_id, result) => {
                if self.cancelling == Some(pairing_id) {
                    self.cancelling = None;
                }
                match result {
                    Ok(pairing) => {
                        if let Some(running) = self.running() {
                            running.ctx.store_mut().apply_pairing(pairing);
                        }
                        if self.route == Route::Pairing(pairing_id) {
                            return self.go(Route::AddDevice);
                        }
                        Task::none()
                    }
                    Err(error) => self.toast(error, None),
                }
            }
            Message::AnswerPairing { pairing_id, accept } => {
                self.answer_pairing(pairing_id, accept)
            }
            Message::PairingAnswered(pairing_id, result) => {
                if self.answering == Some(pairing_id) {
                    self.answering = None;
                }
                match result {
                    Ok(pairing) => {
                        if let Some(running) = self.running() {
                            running.ctx.store_mut().apply_pairing(pairing);
                        }
                    }
                    Err(error) => self.answer_error = Some((pairing_id, error)),
                }
                Task::none()
            }
            Message::CancelTransfer(transfer_id) => self.cancel_transfer(transfer_id),
            Message::OpenFile(path) => self.open(path, false),
            Message::RevealFile(path) => self.open(path, true),
            Message::OpenFailed(path, error) => {
                tracing::warn!(path = %path.display(), %error, "couldn't open a file");
                self.toast(format!("Couldn’t open {}", path.display()), None)
            }
            Message::Rename => self.rename(),
            Message::ChooseDownloadDir => self.choose_download_dir(),
            Message::DownloadDirPicked(Some(folder)) => self.update_settings(SettingsPatch {
                download_dir: Some(Some(folder)),
                ..SettingsPatch::default()
            }),
            Message::DownloadDirPicked(None) => Task::none(),
            Message::SetCloseToTray(enabled) => self.update_settings(SettingsPatch {
                close_to_tray: Some(Some(enabled)),
                ..SettingsPatch::default()
            }),
            Message::SettingsSaved(Ok(settings)) => {
                if let Some(running) = self.running() {
                    running.ctx.store_mut().apply_settings(settings);
                }
                Task::none()
            }
            Message::SettingsSaved(Err(error)) => self.toast(error, None),
            Message::DropSettled(gesture) => match self.drag.settled(gesture) {
                Some(paths) => self.dropped(paths),
                None => Task::none(),
            },
            Message::DropOn(device_id) => self.drop_on(device_id),
            Message::CancelDrop => {
                self.choosing = None;
                Task::none()
            }
            // The pairing prompt can't be dismissed; Escape would only
            // reach a dialog hidden under it.
            Message::Key(KeyCommand::Cancel) if self.incoming_prompt_shows() => Task::none(),
            Message::Key(KeyCommand::Cancel) if self.choosing.is_some() => {
                self.choosing = None;
                Task::none()
            }
            Message::Key(KeyCommand::Cancel) => self.dialog(DialogEvent::Cancel),
            Message::Key(KeyCommand::CloseWindow) => self.close_requested(),
            Message::Key(KeyCommand::Quit) => self.quit(),
            Message::DemoTick(tick) => {
                let Some(running) = self.running() else {
                    return Task::none();
                };
                demo::tick(running.ctx.core(), &running.features, tick);
                self.after(demo::TICK, Message::DemoTick(tick + 1))
            }
            Message::WindowOpened => Task::none(),
            Message::Window(id, event) if Some(id) == self.window => match event {
                window::Event::Focused | window::Event::Unfocused => {
                    self.set_focused(event == window::Event::Focused);
                    Task::none()
                }
                window::Event::CloseRequested => self.close_requested(),
                window::Event::Closed => {
                    self.window_closed();
                    Task::none()
                }
                window::Event::Moved(_) | window::Event::Resized(_) => {
                    self.placement_moves += 1;
                    self.after(
                        PLACEMENT_SAVE_DELAY,
                        Message::SavePlacement(self.placement_moves),
                    )
                }
                // The pairing prompt is modal: a drop would open the
                // chooser under it.
                window::Event::FileHovered(_) if !self.incoming_prompt_shows() => {
                    self.drag.hover();
                    Task::none()
                }
                window::Event::FileDropped(_) | window::Event::FileHovered(_)
                    if self.incoming_prompt_shows() =>
                {
                    self.drag.leave();
                    Task::none()
                }
                window::Event::FilesHoveredLeft => {
                    self.drag.leave();
                    Task::none()
                }
                window::Event::FileDropped(path) => match self.drag.drop_file(path) {
                    Dropped::Wait => Task::none(),
                    Dropped::Settle(gesture) => {
                        self.after(dropping::SETTLE, Message::DropSettled(gesture))
                    }
                    Dropped::Done(paths) => self.dropped(paths),
                },
                _ => Task::none(),
            },
            Message::Window(..) => Task::none(),
        }
    }

    /// Show `route`, and tell the features. Opening Add device from
    /// elsewhere scans, as opening the Flutter page did; coming back to it
    /// from a pairing doesn't.
    fn go(&mut self, route: Route) -> Task<Message> {
        let entering_add_device = route == Route::AddDevice
            && !matches!(self.route, Route::AddDevice | Route::Pairing(_));
        self.route = route;
        let told = match &mut self.phase {
            Phase::Running(running) => running.features.on_route(&running.ctx, &self.route),
            _ => Task::none(),
        };
        if entering_add_device {
            Task::batch([told, self.scan()])
        } else {
            told
        }
    }

    /// Announce this computer so devices nearby answer, and show that the
    /// page is searching.
    fn scan(&mut self) -> Task<Message> {
        let Some(running) = self.running() else {
            return Task::none();
        };
        let announced = running.ctx.core().announce();
        let searching = self.show_searching();
        match announced {
            Ok(()) => searching,
            Err(error) => Task::batch([searching, self.toast(error::describe_error(&error), None)]),
        }
    }

    fn show_searching(&mut self) -> Task<Message> {
        self.scan += 1;
        self.searching = true;
        self.after(
            add_device::SEARCH_INDICATOR,
            Message::SearchEnded(self.scan),
        )
    }

    /// Ask for an IP address and announce this computer to it. The dialog
    /// stays open with the reason until the address is accepted, then the
    /// page shows it is searching.
    fn add_by_address(&mut self) -> Task<Message> {
        let Some(running) = self.running() else {
            return Task::none();
        };
        let core = running.ctx.core().clone();
        self.dialogs.open(Dialog::prompt(
            "Add by IP address",
            Field {
                label: Some("IP address".into()),
                hint: Some("192.168.1.20".into()),
                helper: Some(
                    "MyConnect or KDE Connect must be running on that device. It appears \
                         in the list once it answers."
                        .into(),
                ),
                ..Field::default()
            },
            "Add",
            Submit::Run(Arc::new(move |address| {
                Task::done(announce_to(&core, &address).map(|()| Message::ShowSearching))
            })),
        ))
    }

    /// Start pairing with `device_id` on the daemon's runtime, which times
    /// the request out.
    fn pair(&mut self, device_id: String) -> Task<Message> {
        if self.starting.is_some() {
            return Task::none();
        }
        let Some(running) = self.running() else {
            return Task::none();
        };
        let core = running.ctx.core().clone();
        self.starting = Some(device_id.clone());
        self.core_task(
            move || core.start_outgoing_pairing(&device_id),
            Message::PairStarted,
        )
    }

    fn cancel_pairing(&mut self, pairing_id: Uuid) -> Task<Message> {
        let Some(running) = self.running() else {
            return Task::none();
        };
        let core = running.ctx.core().clone();
        self.cancelling = Some(pairing_id);
        self.core_task(
            move || core.cancel_pairing(pairing_id),
            move |result| Message::PairingCancelled(pairing_id, result),
        )
    }

    /// Accept (which writes the trust store) or reject an incoming request.
    fn answer_pairing(&mut self, pairing_id: Uuid, accept: bool) -> Task<Message> {
        if self.answering.is_some() {
            return Task::none();
        }
        let Some(running) = self.running() else {
            return Task::none();
        };
        let core = running.ctx.core().clone();
        self.answering = Some(pairing_id);
        self.answer_error = None;
        self.core_task(
            move || {
                if accept {
                    core.accept_pairing(pairing_id)
                } else {
                    core.cancel_pairing(pairing_id)
                }
            },
            move |result| Message::PairingAnswered(pairing_id, result),
        )
    }

    /// Ask a transfer to stop. Its task tears it down and marks it
    /// cancelled a moment later; the event says so.
    fn cancel_transfer(&mut self, transfer_id: Uuid) -> Task<Message> {
        let Some(running) = self.running() else {
            return Task::none();
        };
        match running.ctx.core().cancel_transfer(transfer_id) {
            Ok(transfer) => {
                running.ctx.store_mut().apply_transfer(transfer);
                Task::none()
            }
            Err(error) => self.toast(error::describe_error(&error), None),
        }
    }

    /// Open `path`, or show it in the file manager (`reveal`), off the UI
    /// thread: it spawns a process or calls D-Bus.
    fn open(&self, path: PathBuf, reveal: bool) -> Task<Message> {
        let opener = self.desktop.opener.clone();
        context::on_runtime(&self.options.runtime, async move {
            let result = tokio::task::spawn_blocking({
                let path = path.clone();
                move || {
                    if reveal {
                        opener.reveal(&path)
                    } else {
                        opener.open(&path)
                    }
                }
            })
            .await
            .unwrap_or_else(|error| Err(error.to_string()));
            result.err().map(|error| Message::OpenFailed(path, error))
        })
        .and_then(Task::done)
    }

    /// Ask for a new name for this computer. The dialog stays open, with
    /// the daemon's objection under the field, until a name is accepted.
    fn rename(&mut self) -> Task<Message> {
        let Some(running) = self.running() else {
            return Task::none();
        };
        let Some(current) = running.ctx.store().settings().loaded() else {
            return Task::none();
        };
        let current = current.device_name.clone();
        let core = running.ctx.core().clone();
        let runtime = self.options.runtime.clone();
        self.dialogs.open(Dialog::prompt(
            "Device name",
            Field {
                value: current,
                helper: Some("How this computer appears on your other devices".into()),
                max_len: Some(32),
                ..Field::default()
            },
            "Save",
            Submit::Run(Arc::new(move |name| {
                let core = core.clone();
                context::on_runtime(&runtime, async move {
                    core.update_settings(SettingsPatch {
                        device_name: Some(Some(name)),
                        ..SettingsPatch::default()
                    })
                    .map(|settings| Message::SettingsSaved(Ok(settings)))
                    .map_err(|error| error::describe_error(&error))
                })
            })),
        ))
    }

    /// Open the folder picker at the current download folder. It isn't
    /// disabled meanwhile, as in Flutter: a portal that never answers would
    /// otherwise lock the setting until the app restarts.
    fn choose_download_dir(&self) -> Task<Message> {
        let Phase::Running(running) = &self.phase else {
            return Task::none();
        };
        let Some(settings) = running.ctx.store().settings().loaded() else {
            return Task::none();
        };
        let picked = self
            .desktop
            .picker
            .pick_folder("Save received files in", &settings.download_dir);
        Task::future(picked).map(Message::DownloadDirPicked)
    }

    /// Change settings on the daemon's runtime (it writes the settings
    /// file), applying the answer or saying why it failed.
    fn update_settings(&mut self, patch: SettingsPatch) -> Task<Message> {
        let Some(running) = self.running() else {
            return Task::none();
        };
        let core = running.ctx.core().clone();
        self.core_task(move || core.update_settings(patch), Message::SettingsSaved)
    }

    /// Run a core call on the daemon's runtime, with its error in words.
    fn core_task<T: Send + 'static>(
        &self,
        call: impl FnOnce() -> Result<T, CoreError> + Send + 'static,
        then: impl Fn(Result<T, String>) -> Message + Send + 'static,
    ) -> Task<Message> {
        context::on_runtime(&self.options.runtime, async move {
            call().map_err(|error| error::describe_error(&error))
        })
        .map(then)
    }

    /// Files were dropped on the window: send them where the page says, if
    /// a feature takes them there, or ask where. Folders are refused.
    fn dropped(&mut self, paths: Vec<PathBuf>) -> Task<Message> {
        if self.running().is_none() {
            return Task::none();
        }
        // Folders can't be sent, and some drops aren't local files.
        let files: Vec<_> = paths
            .iter()
            .filter(|path| path.is_file())
            .cloned()
            .collect();
        if files.is_empty() {
            if paths.is_empty() {
                return Task::none();
            }
            return self.toast("Only files can be sent, not folders.".into(), None);
        }
        match self.drop_target_here() {
            Some(target) => Task::done(Message::Feature((target.on_drop)(files), Origin::Window)),
            None => {
                self.choosing = Some(files);
                Task::none()
            }
        }
    }

    /// The chooser's device was picked: open its page and hand it the
    /// files.
    fn drop_on(&mut self, device_id: String) -> Task<Message> {
        let Some(files) = self.choosing.take() else {
            return Task::none();
        };
        let route = Route::Device(device_id.clone());
        let target = self.drop_target(&device_id, &route);
        let go = self.go(route);
        let then = match target {
            Some(target) => Task::done(Message::Feature((target.on_drop)(files), Origin::Window)),
            // It dropped out of the list as it was chosen.
            None => self.toast(error::describe_code("device_not_connected"), None),
        };
        Task::batch([go, then])
    }

    /// Where a drop on the current page goes, if a feature takes it.
    fn drop_target_here(&self) -> Option<DropTarget> {
        self.drop_target(self.route.device()?, &self.route)
    }

    /// What files dropped on `device_id` while the window shows `route`
    /// would do, if a feature takes them.
    fn drop_target(&self, device_id: &str, route: &Route) -> Option<DropTarget> {
        let Phase::Running(running) = &self.phase else {
            return None;
        };
        let device = running.ctx.device(device_id)?;
        running.features.drop_target(device, route)
    }

    /// Whether an incoming pairing request is waiting for the user.
    fn incoming_prompt_shows(&self) -> bool {
        match &self.phase {
            Phase::Running(running) => !running.ctx.store().pending_incoming_pairings().is_empty(),
            _ => false,
        }
    }

    /// Unpair `device_id`, off the UI thread: it writes the trust store.
    fn forget(&mut self, device_id: String) -> Task<Message> {
        let Some(running) = self.running() else {
            return Task::none();
        };
        let core = running.ctx.core().clone();
        self.unpairing = Some(device_id.clone());
        context::on_runtime(&self.options.runtime, {
            let device_id = device_id.clone();
            async move {
                core.forget_device(&device_id)
                    .map_err(|error| error::describe_error(&error))
            }
        })
        .map(move |result| Message::Forgotten {
            device_id: device_id.clone(),
            result,
        })
    }

    fn dialog(&mut self, event: DialogEvent) -> Task<Message> {
        let closes = matches!(event, DialogEvent::Cancel);
        match self.dialogs.update(event) {
            Step::Nothing if closes => self.dialogs.focus(),
            Step::Nothing => Task::none(),
            Step::Send(message) => Task::batch([self.dialogs.focus(), Task::done(message)]),
            Step::Run(id, work) => {
                work.map(move |result| Message::DialogFinished(id, result.map(Box::new)))
            }
        }
    }

    /// Hand `feature` to its feature. What it asks of the shell is shown
    /// as `origin` suits.
    fn feature(&mut self, feature: Feature, origin: Origin) -> Task<Message> {
        let Some(running) = self.running() else {
            return Task::none();
        };
        running.features.update(&running.ctx, feature, origin)
    }

    /// Show the window for a request from the tray that needs it (a
    /// dialog); from the window, nothing.
    fn show_window_for(&mut self, origin: Origin) -> Task<Message> {
        match origin {
            Origin::Window => Task::none(),
            Origin::Tray => self.show_window(),
        }
    }

    fn prompt(&mut self, prompt: shell::Prompt) -> Task<Message> {
        let shell::Prompt {
            title,
            label,
            initial,
            selection,
            confirm_label,
            validate,
            then,
            origin,
        } = prompt;
        let show = self.show_window_for(origin);
        let open = self.dialogs.open(Dialog::prompt(
            title,
            Field {
                value: initial,
                label: Some(label),
                validate: Some(validate),
                selection,
                ..Field::default()
            },
            confirm_label,
            Submit::Close(Arc::new(move |text| {
                Message::Feature(then(text), Origin::Window)
            })),
        ));
        Task::batch([show, open])
    }

    /// rfd can't relabel the confirm button, so it is the platform's own
    /// word ("Open"). The files go back as coming from where the picker
    /// was asked for.
    fn pick_files(&self, pick: shell::PickFiles) -> Task<Message> {
        let shell::PickFiles {
            title,
            then,
            origin,
        } = pick;
        Task::future(self.desktop.picker.pick_files(&title)).and_then(move |paths| {
            if paths.is_empty() {
                return Task::none();
            }
            Task::done(Message::Feature(then(paths), origin))
        })
    }

    /// Tell the user something: in a toast over a focused window,
    /// otherwise in a desktop notification.
    fn notify(&mut self, title: &str, body: &str) -> Task<Message> {
        if self.focused() {
            return self.toast(format!("{title}: {body}"), None);
        }
        self.notifications
            .show(&*self.desktop.notifier, title, body);
        Task::none()
    }

    /// Notify about files received while the window isn't in front.
    fn notify_received(&mut self, received: Vec<(String, String)>) {
        if self.focused() {
            return;
        }
        for (file, device) in received {
            self.notifications.show(
                &*self.desktop.notifier,
                "File received",
                &format!("{file} from {device}"),
            );
        }
    }

    /// Follow the pending pairing requests with notifications.
    fn notify_pairings(&mut self) {
        let focused = self.focused();
        let Phase::Running(running) = &self.phase else {
            return;
        };
        let pending = running.ctx.store().pending_incoming_pairings();
        self.notifications
            .pairings(&*self.desktop.notifier, &pending, focused);
    }

    /// Send the tray the menu for now, if it looks different.
    fn update_tray(&mut self) {
        let running = match &self.phase {
            Phase::Running(running) => Some((running.ctx.store(), &running.features)),
            _ => None,
        };
        let menu = background::tray_menu(running);
        if trays::layout(&menu) != trays::layout(&self.tray_menu) {
            self.desktop.tray.set_menu(menu.clone());
        }
        self.tray_menu = menu;
    }

    fn desktop_event(&mut self, event: DesktopEvent) -> Task<Message> {
        match event {
            DesktopEvent::TrayClicked
            | DesktopEvent::NotificationClicked
            | DesktopEvent::ShowRequested => self.show_window(),
            DesktopEvent::TrayChose(command) => self.tray_command(command),
            DesktopEvent::TrayAvailable(available) => {
                self.desktop.tray_available = available;
                // Nothing else could bring a closed window back.
                if !available && self.window.is_none() {
                    return self.show_window();
                }
                Task::none()
            }
            DesktopEvent::QuitRequested => self.quit(),
        }
    }

    fn tray_command(&mut self, command: TrayCommand) -> Task<Message> {
        match command {
            TrayCommand::Open => self.show_window(),
            TrayCommand::Settings => Task::batch([self.go(Route::Settings), self.show_window()]),
            TrayCommand::ShowDevice(device_id) => {
                Task::batch([self.go(Route::Device(device_id)), self.show_window()])
            }
            TrayCommand::Quit => self.quit(),
            TrayCommand::Action(feature) => self.feature(feature, Origin::Tray),
        }
    }

    /// Whether the window is open and in front of the user.
    fn focused(&self) -> bool {
        self.window.is_some() && self.window_focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.window_focused = focused;
        let focused = self.focused();
        if let Some(running) = self.running() {
            running.ctx.set_window_focused(focused);
        }
    }

    /// Show, raise and focus the window, opening it where it was if it is
    /// closed.
    fn show_window(&mut self) -> Task<Message> {
        match self.window {
            Some(id) => self.desktop.windows.raise(id).discard(),
            None => self.open_window(),
        }
    }

    fn open_window(&mut self) -> Task<Message> {
        let settings = windowing::settings(&self.placement, &self.desktop.windows.screens());
        let (id, opened) = self.desktop.windows.open(settings);
        self.window = Some(id);
        // It opens in front; `Unfocused` says if not.
        self.set_focused(true);
        if !self.placement.visible {
            self.placement.visible = true;
            self.save_placement();
        }
        opened.map(|()| Message::WindowOpened)
    }

    /// The window's close button: close to the tray, or quit if the user
    /// doesn't want the app to keep running or there is no tray. Without
    /// settings (the daemon didn't start), close to the tray, so it stays
    /// the way out.
    fn close_requested(&mut self) -> Task<Message> {
        let Some(id) = self.window else {
            return Task::none();
        };
        let close_to_tray = match &self.phase {
            Phase::Running(running) => running
                .ctx
                .store()
                .settings()
                .loaded()
                .is_none_or(|settings| settings.close_to_tray),
            _ => true,
        };
        if close_to_tray && self.desktop.tray_available {
            // Where it is, to open it there again.
            self.desktop.windows.read(id).map(Message::Hide)
        } else {
            self.quit()
        }
    }

    /// Close the window to the tray, keeping where it was.
    fn hide(&mut self, seen: Seen) -> Task<Message> {
        let Some(id) = self.window else {
            return Task::none();
        };
        if self.quitting {
            return Task::none();
        }
        self.placement = self.placement.seen(seen, false);
        self.save_placement();
        self.window_closed();
        self.desktop.windows.close(id).discard()
    }

    fn window_closed(&mut self) {
        self.window = None;
        self.set_focused(false);
        self.drag.leave();
    }

    /// Quit: save where the window is, then end the UI; [`run`] shuts the
    /// daemon down after. Only once, whoever asks.
    fn quit(&mut self) -> Task<Message> {
        if self.quitting {
            return Task::none();
        }
        self.quitting = true;
        match self.window {
            Some(id) => Task::batch([
                self.desktop
                    .windows
                    .read(id)
                    .map(|seen| Message::Exit(Some(seen))),
                // In case the window can't be read any more.
                self.after(Duration::from_secs(1), Message::Exit(None)),
            ]),
            None => Task::done(Message::Exit(None)),
        }
    }

    fn exit(&mut self, seen: Option<Seen>) -> Task<Message> {
        match seen {
            Some(seen) if self.window.is_some() => {
                self.placement = self.placement.seen(seen, true);
            }
            _ => self.placement.visible = self.window.is_some(),
        }
        self.save_placement();
        iced::exit()
    }

    /// Keep the placement for the next launch. It is a few bytes, written
    /// in place of blocking on a task at exit.
    fn save_placement(&self) {
        if let Some(placements) = &self.desktop.placements {
            placements.save(self.placement);
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
        context::on_runtime(&self.options.runtime, async move {
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
        let page = match self.drop_hint() {
            Some(label) => dropping::hint(page, label),
            None => page,
        };
        let page = self.dialogs.view(page, Message::Dialog);
        let page = match self.chooser() {
            Some(chooser) => dialogs::modal(page, chooser, Some(Message::CancelDrop)),
            None => page,
        };
        match self.incoming_prompt() {
            Some(prompt) => dialogs::modal(page, prompt, None),
            None => page,
        }
    }

    /// What the drop hint says while files hover over the window, if they
    /// do.
    fn drop_hint(&self) -> Option<String> {
        if !self.drag.is_active() || !matches!(self.phase, Phase::Running(_)) {
            return None;
        }
        Some(
            self.drop_target_here()
                .map_or_else(|| dropping::CHOOSE_LABEL.into(), |target| target.label),
        )
    }

    /// The chooser for dropped files, listing the paired devices a feature
    /// would send them to now. It follows devices as they come and go.
    fn chooser(&self) -> Option<Element<'_, Message>> {
        let files = self.choosing.as_ref()?;
        Some(dropping::chooser(
            files,
            self.recipients(),
            Message::DropOn,
            Message::CancelDrop,
        ))
    }

    /// The paired devices that would take dropped files now, by name.
    fn recipients(&self) -> Vec<&crate::core::DeviceSnapshot> {
        let Phase::Running(running) = &self.phase else {
            return Vec::new();
        };
        running
            .ctx
            .store()
            .paired_devices()
            .into_loaded()
            .unwrap_or_default()
            .into_iter()
            .filter(|device| {
                self.drop_target(&device.device_id, &Route::Device(device.device_id.clone()))
                    .is_some()
            })
            .collect()
    }

    /// The prompt for the oldest incoming pairing request, over everything.
    fn incoming_prompt(&self) -> Option<Element<'_, Message>> {
        let Phase::Running(running) = &self.phase else {
            return None;
        };
        let pending = running.ctx.store().pending_incoming_pairings();
        let first = pending.first()?.id;
        let error = self
            .answer_error
            .as_ref()
            .filter(|(id, _)| *id == first)
            .map(|(_, error)| error.as_str());
        incoming::view(
            &pending,
            self.answering.is_some(),
            error,
            &incoming::Actions {
                accept: |pairing_id| Message::AnswerPairing {
                    pairing_id,
                    accept: true,
                },
                reject: |pairing_id| Message::AnswerPairing {
                    pairing_id,
                    accept: false,
                },
            },
        )
    }

    /// The page for the current route.
    fn page<'a>(&'a self, running: &'a Running) -> Element<'a, Message> {
        let navigate = |route: Route| Message::Navigate(route, Origin::Window);
        match &self.route {
            Route::Devices => devices::view(
                running.ctx.store(),
                &|device| running.features.device_statuses(device),
                navigate,
                Message::Reload,
            ),
            Route::Browse { device, folder } => match running.ctx.device(device) {
                Some(device) => running.features.browse_page(device, folder.clone()),
                None => widgets::page(
                    widgets::page_header("", Some(Message::Back), vec![]),
                    widgets::empty_state(
                        lucide::circle_alert,
                        "This device is no longer known.",
                        None,
                        None,
                    ),
                ),
            },
            Route::Device(id) => device::view(
                running.ctx.store(),
                &device::DeviceFeatures {
                    statuses: &|device| running.features.device_statuses(device),
                    actions: &|device| running.features.device_actions(device),
                },
                id,
                self.unpairing.as_deref() == Some(id.as_str()),
                &device::Actions {
                    navigate,
                    feature: |feature| Message::Feature(feature, Origin::Window),
                    unpair: |device| Message::Unpair {
                        device_id: device.device_id.clone(),
                        name: device.device_name.clone(),
                    },
                    transfer: TRANSFER_ACTIONS,
                },
            ),
            Route::AddDevice => add_device::view(
                running.ctx.store(),
                self.searching,
                self.starting.as_deref(),
                add_device::Actions {
                    back: Message::Back,
                    scan: Message::Scan,
                    add_by_address: Message::AddByAddress,
                    retry: Message::Reload,
                    pair: Message::Pair,
                },
            ),
            Route::Pairing(id) => pairing::view(
                running.ctx.store(),
                *id,
                self.starting.is_some() || self.cancelling == Some(*id),
                pairing::Actions {
                    navigate,
                    cancel: Message::CancelPairing,
                    retry: Message::Pair,
                },
            ),
            Route::Transfers => transfers::view(
                running.ctx.store(),
                &TRANSFER_ACTIONS,
                Message::Back,
                Message::Reload,
            ),
            Route::Settings => settings::view(
                running.ctx.store(),
                |settings| running.features.settings_sections(settings),
                &self.options.version,
                settings::Actions {
                    back: Message::Back,
                    retry: Message::Reload,
                    rename: Message::Rename,
                    choose_download_dir: Message::ChooseDownloadDir,
                    set_close_to_tray: Message::SetCloseToTray,
                },
            ),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            window::events().map(|(id, event)| Message::Window(id, event)),
            event::listen_with(key_command).map(Message::Key),
        ];
        if let Some(events) = &self.desktop.events {
            subscriptions.push(desktop::events(events).map(Message::Desktop));
        }
        if let Phase::Running(running) = &self.phase {
            subscriptions.push(sync::watch(running.ctx.core()).map(Message::Sync));
            subscriptions.push(running.features.subscription());
        }
        Subscription::batch(subscriptions)
    }
}

/// What a transfer row's buttons ask of the shell.
const TRANSFER_ACTIONS: transfers::Actions<Message> = transfers::Actions {
    cancel: Message::CancelTransfer,
    open: Message::OpenFile,
    reveal: Message::RevealFile,
};

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

/// Announce this computer to `address`, typed by the user.
fn announce_to(core: &Core, address: &str) -> Result<(), String> {
    let address: Ipv4Addr = address
        .trim()
        .parse()
        .map_err(|_| error::describe_code("invalid_address"))?;
    core.announce_to(address)
        .map_err(|error| error::describe_error(&error))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use iced::widget;
    use iced_test::simulator::Simulator;

    use super::*;
    use crate::{
        core::{LanCommand, PairingDirection, PairingStatus, TransferDirection, testing::handle},
        plugins::clipboard::InMemoryClipboard,
        ui::features::{browse, ping},
    };

    /// The tray's menus, as the shell sent them.
    #[derive(Default)]
    struct FakeTray {
        menu: Mutex<Vec<TrayItem>>,
        updates: AtomicUsize,
    }

    impl Tray for FakeTray {
        fn set_menu(&self, menu: Vec<TrayItem>) {
            *self.menu.lock().unwrap() = menu;
            self.updates.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// The notifications showing, by id: their title and body.
    #[derive(Default)]
    struct FakeNotifier {
        shown: Mutex<std::collections::BTreeMap<u32, (String, String)>>,
    }

    impl Notifier for FakeNotifier {
        fn show(&self, id: u32, title: &str, body: &str) {
            self.shown
                .lock()
                .unwrap()
                .insert(id, (title.into(), body.into()));
        }

        fn withdraw(&self, id: u32) {
            self.shown.lock().unwrap().remove(&id);
        }
    }

    /// Windows that open and close at once, and are seen at one spot.
    struct FakeWindows {
        opened: Mutex<Vec<window::Settings>>,
        raised: AtomicUsize,
        seen: Seen,
    }

    impl Default for FakeWindows {
        fn default() -> Self {
            Self {
                opened: Mutex::default(),
                raised: AtomicUsize::new(0),
                seen: Seen {
                    position: Some(iced::Point::new(30.0, 40.0)),
                    size: iced::Size::new(500.0, 700.0),
                    maximized: false,
                    minimized: false,
                },
            }
        }
    }

    impl Windows for FakeWindows {
        fn open(&self, settings: window::Settings) -> (window::Id, Task<()>) {
            self.opened.lock().unwrap().push(settings);
            (window::Id::unique(), Task::none())
        }

        fn close(&self, _id: window::Id) -> Task<()> {
            Task::none()
        }

        fn raise(&self, _id: window::Id) -> Task<()> {
            self.raised.fetch_add(1, Ordering::SeqCst);
            Task::none()
        }

        fn read(&self, _id: window::Id) -> Task<Seen> {
            Task::done(self.seen)
        }

        fn screens(&self) -> Vec<iced::Rectangle> {
            vec![iced::Rectangle::new(
                iced::Point::ORIGIN,
                iced::Size::new(1920.0, 1080.0),
            )]
        }
    }

    /// The desktop an app is given in tests, to look at afterwards.
    #[derive(Clone, Default)]
    struct Fakes {
        tray: Arc<FakeTray>,
        notifier: Arc<FakeNotifier>,
        windows: Arc<FakeWindows>,
        /// Where `window.json` goes, if the test keeps one.
        placements: Option<PlacementStore>,
        /// No tray host shows the icon.
        no_tray: bool,
    }

    impl Fakes {
        fn desktop(&self) -> Desktop {
            Desktop {
                tray: self.tray.clone(),
                tray_available: !self.no_tray,
                notifier: self.notifier.clone(),
                windows: self.windows.clone(),
                placements: self.placements.clone(),
                events: None,
                opener: Arc::new(opening::System),
                picker: Arc::new(picking::System),
            }
        }

        /// The bodies of the notifications showing.
        fn notified(&self) -> Vec<String> {
            self.notifier
                .shown
                .lock()
                .unwrap()
                .values()
                .map(|(_, body)| body.clone())
                .collect()
        }

        fn menu(&self) -> Vec<TrayItem> {
            self.tray.menu.lock().unwrap().clone()
        }
    }

    /// An app whose daemon never starts: tests set the phase themselves.
    fn app(runtime: tokio::runtime::Handle) -> App {
        app_on(runtime, &Fakes::default())
    }

    fn app_on(runtime: tokio::runtime::Handle, fakes: &Fakes) -> App {
        let start: Rc<dyn Fn() -> StartFuture> = Rc::new(|| Box::pin(std::future::pending()));
        App::boot(
            UiOptions {
                runtime,
                demo: false,
                version: "1.2.3 (test)".into(),
                data_dir: None,
            },
            start,
            Service::default(),
            fakes.desktop(),
        )
        .0
    }

    /// The features, over clipboard and browse plugins of their own, which
    /// the test's core doesn't run.
    fn features() -> Features {
        Features::new(
            Arc::new(ClipboardPlugin::new(InMemoryClipboard::shared())),
            Arc::new(BrowsePlugin::default()),
        )
    }

    /// An app over a test core.
    fn running() -> App {
        running_with_commands().0
    }

    /// An app over a test core, and what the core asks of the network.
    fn running_with_commands() -> (App, tokio::sync::mpsc::Receiver<LanCommand>) {
        let (core, commands) = handle();
        (running_on(core), commands)
    }

    /// An app over `core`.
    fn running_on(core: Core) -> App {
        running_on_desktop(core, &Fakes::default())
    }

    /// An app over `core`, on `fakes`.
    fn running_on_desktop(core: Core, fakes: &Fakes) -> App {
        running_with(core, features(), fakes)
    }

    /// An app running `features` over `core`, on `fakes`.
    fn running_with(core: Core, features: Features, fakes: &Fakes) -> App {
        let runtime = tokio::runtime::Handle::current();
        let mut app = app_on(runtime.clone(), fakes);
        app.phase = Phase::Running(Box::new(Running {
            ctx: UiContext::new(core, runtime),
            features,
        }));
        app
    }

    fn features_mut(app: &mut App) -> &mut Features {
        let Phase::Running(running) = &mut app.phase else {
            panic!("the app runs");
        };
        &mut running.features
    }

    fn core(app: &App) -> Core {
        let Phase::Running(running) = &app.phase else {
            panic!("the app runs");
        };
        running.ctx.core().clone()
    }

    fn store(app: &App) -> &store::Store {
        let Phase::Running(running) = &app.phase else {
            panic!("the app runs");
        };
        running.ctx.store()
    }

    /// Run `message` through `app` and return what it leads to, without
    /// running those.
    async fn step(app: &mut App, message: Message) -> Vec<Message> {
        testing::outputs(app.update(message)).await
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
    async fn a_feature_can_toast_and_navigate() {
        let mut app = running();
        settle(
            &mut app,
            Message::Toast {
                text: "Opening".into(),
                action: None,
                origin: Origin::Window,
            },
        )
        .await;
        let browse = Feature::Browse(browse::Message::Browse {
            device_id: "phone".into(),
        });
        settle(&mut app, Message::Feature(browse, Origin::Window)).await;

        assert_eq!(app.toasts.items().len(), 1);
        assert_eq!(app.toasts.items()[0].text, "Opening");
        assert_eq!(
            app.route,
            Route::Browse {
                device: "phone".into(),
                folder: None
            }
        );
        let _ = app.update(Message::DismissToast(app.toasts.items()[0].id));
        assert!(app.toasts.is_empty());
    }

    #[tokio::test]
    async fn back_goes_to_the_parent_page() {
        let mut app = running();
        app.route = Route::Browse {
            device: "phone".into(),
            folder: Some("/storage/emulated/0".into()),
        };
        let _ = app.update(Message::Back);
        assert_eq!(app.route, Route::Device("phone".into()));
        let _ = app.update(Message::Back);
        let _ = app.update(Message::Back);
        assert_eq!(app.route, Route::Devices);
    }

    #[tokio::test]
    async fn a_toast_button_goes_to_its_page_and_dismisses_it() {
        let mut app = running();
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
    async fn a_feature_confirm_sends_its_message_only_when_confirmed() {
        let (core, _commands) = handle();
        let (peer, mut sent) = testing::connect_peer(
            &core,
            testing::PEER_ID,
            &[crate::plugins::ping::PACKET_TYPE],
        );
        let mut app = running_on(core);
        let confirm = || Message::Confirm {
            title: "Ping it?".into(),
            body: "It will hear you.".into(),
            confirm_label: "Ping".into(),
            then: Feature::Ping(ping::Message::Ping {
                device_id: peer.device_id.clone(),
                name: peer.device_name.clone(),
            }),
            origin: Origin::Window,
        };

        settle(&mut app, confirm()).await;
        assert_eq!(app.dialogs.current().unwrap().title, "Ping it?");
        settle(&mut app, Message::Key(KeyCommand::Cancel)).await;
        assert!(app.dialogs.current().is_none());
        assert!(sent.try_recv().is_err(), "not sent");

        settle(&mut app, confirm()).await;
        settle(&mut app, Message::Dialog(DialogEvent::Submit)).await;
        assert!(app.dialogs.current().is_none());
        assert!(sent.try_recv().is_ok(), "sent");
    }

    #[tokio::test(start_paused = true)]
    async fn unpairing_asks_then_forgets_the_device_and_goes_home() {
        let mut app = running();
        let Phase::Running(running) = &app.phase else {
            unreachable!()
        };
        let core = running.ctx.core().clone();
        let (peer, _sent) = testing::connect_peer(&core, testing::PEER_ID, &[]);
        settle(&mut app, Message::Reload).await;
        app.route = Route::Device(peer.device_id.clone());
        let unpair = || Message::Unpair {
            device_id: peer.device_id.clone(),
            name: peer.device_name.clone(),
        };

        // Cancelling keeps the device.
        settle(&mut app, unpair()).await;
        assert_eq!(app.dialogs.current().unwrap().title, "Unpair Peer?");
        settle(&mut app, Message::Key(KeyCommand::Cancel)).await;
        assert!(core.device(&peer.device_id).is_some());

        settle(&mut app, unpair()).await;
        settle(&mut app, Message::Dialog(DialogEvent::Submit)).await;
        assert!(app.dialogs.current().is_none());
        assert!(core.device(&peer.device_id).is_none(), "the core forgot it");
        let Phase::Running(running) = &app.phase else {
            unreachable!()
        };
        assert!(
            running.ctx.device(&peer.device_id).is_none(),
            "gone from the store without waiting for the event"
        );
        assert_eq!(app.route, Route::Devices);
        assert!(app.unpairing.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_unpair_says_why_and_stays() {
        let mut app = running();
        app.route = Route::Device("gone".into());
        settle(
            &mut app,
            Message::Forget {
                device_id: "gone".into(),
            },
        )
        .await;
        assert_eq!(app.route, Route::Device("gone".into()));
        assert_eq!(
            app.toasts.items()[0].text,
            "That device is no longer known."
        );
        assert!(app.unpairing.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn opening_add_device_scans_and_searches_for_a_while() {
        let (mut app, mut commands) = running_with_commands();
        let ended = step(
            &mut app,
            Message::Navigate(Route::AddDevice, Origin::Window),
        )
        .await;
        assert_eq!(app.route, Route::AddDevice);
        assert_eq!(commands.try_recv().unwrap(), LanCommand::AnnounceDiscovery);
        assert!(app.searching);
        // The timer ran out while the test waited for it.
        let [Message::SearchEnded(_)] = &ended[..] else {
            panic!("unexpected outputs: {ended:?}");
        };
        let _ = app.update(ended.into_iter().next().unwrap());
        assert!(!app.searching);

        // A scan's timer doesn't end a later scan's search.
        let first = step(&mut app, Message::Scan).await;
        let _ = app.update(Message::Scan);
        for message in first {
            let _ = app.update(message);
        }
        assert!(app.searching, "the second scan is still searching");

        // Coming back from a pairing doesn't scan again.
        while commands.try_recv().is_ok() {}
        app.route = Route::Pairing(Uuid::nil());
        let _ = app.update(Message::Back);
        assert_eq!(app.route, Route::AddDevice);
        assert!(commands.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn add_by_ip_says_why_an_address_is_refused_then_announces_to_it() {
        let (mut app, mut commands) = running_with_commands();
        app.route = Route::AddDevice;
        settle(&mut app, Message::AddByAddress).await;
        assert_eq!(app.dialogs.current().unwrap().title, "Add by IP address");

        for refused in ["300.1.1.1", "255.255.255.255"] {
            settle(
                &mut app,
                Message::Dialog(DialogEvent::Input(refused.into())),
            )
            .await;
            settle(&mut app, Message::Dialog(DialogEvent::Submit)).await;
            let dialog = app.dialogs.current().expect("the dialog stays open");
            assert_eq!(
                dialog.error(),
                Some("Enter an IPv4 address, like 192.168.1.20."),
                "{refused}"
            );
            assert!(!dialog.is_busy());
        }
        assert!(commands.try_recv().is_err());

        settle(
            &mut app,
            Message::Dialog(DialogEvent::Input(" 192.168.1.20 ".into())),
        )
        .await;
        let finished = step(&mut app, Message::Dialog(DialogEvent::Submit)).await;
        let mut then = Vec::new();
        for message in finished {
            then.extend(step(&mut app, message).await);
        }
        assert!(app.dialogs.current().is_none());
        assert_eq!(
            commands.try_recv().unwrap(),
            LanCommand::AnnounceTo {
                address: Ipv4Addr::new(192, 168, 1, 20)
            }
        );
        assert!(matches!(then[..], [Message::ShowSearching]));
        let _ = app.update(Message::ShowSearching);
        assert!(app.searching, "the page searches for the device");
    }

    #[tokio::test(start_paused = true)]
    async fn pairing_opens_the_request_and_cancelling_returns_to_add_device() {
        let mut app = running();
        let core = core(&app);
        let (peer, mut sent) = testing::connect_unpaired_peer(&core, testing::PEER_ID);
        settle(&mut app, Message::Reload).await;
        app.route = Route::AddDevice;

        settle(&mut app, Message::Pair(peer.device_id.clone())).await;
        let Route::Pairing(pairing_id) = app.route else {
            panic!("the pairing page shows, not {:?}", app.route);
        };
        assert!(app.starting.is_none());
        let pairing = store(&app).pairing(pairing_id).unwrap();
        assert_eq!(pairing.status, PairingStatus::AwaitingConfirmation);
        assert!(pairing.verification_code.is_some());
        assert_eq!(sent.try_recv().unwrap().packet_type, "kdeconnect.pair");

        settle(&mut app, Message::CancelPairing(pairing_id)).await;
        assert_eq!(app.route, Route::AddDevice);
        assert!(app.cancelling.is_none());
        assert_eq!(
            store(&app).pairing(pairing_id).unwrap().status,
            PairingStatus::Rejected
        );

        // Try again starts a new request and shows it.
        app.route = Route::Pairing(pairing_id);
        settle(&mut app, Message::Pair(peer.device_id.clone())).await;
        assert!(matches!(app.route, Route::Pairing(id) if id != pairing_id));
    }

    #[tokio::test(start_paused = true)]
    async fn a_pairing_that_cant_start_says_why_and_stays() {
        let mut app = running();
        app.route = Route::AddDevice;
        settle(&mut app, Message::Pair("gone".into())).await;
        assert_eq!(app.route, Route::AddDevice);
        assert_eq!(
            app.toasts.items()[0].text,
            "That device is no longer known."
        );
        assert!(app.starting.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn the_incoming_prompt_shows_on_any_page_until_resolved() {
        let mut app = running();
        let core = core(&app);
        let (peer, _sent) = testing::connect_unpaired_peer(&core, testing::PEER_ID);
        testing::request_pairing(&core, &peer.device_id);
        settle(&mut app, Message::Reload).await;

        for route in [
            Route::Devices,
            Route::Settings,
            Route::Transfers,
            Route::AddDevice,
            Route::Device(peer.device_id.clone()),
        ] {
            app.route = route;
            assert!(app.incoming_prompt().is_some(), "{:?}", app.route);
        }

        // Resolved elsewhere (the CLI): the prompt goes on its own.
        let request = store(&app).pending_incoming_pairings()[0].id;
        core.cancel_pairing(request).unwrap();
        settle(&mut app, Message::Reload).await;
        assert!(app.incoming_prompt().is_none());

        // Accepting pairs the device.
        testing::request_pairing(&core, &peer.device_id);
        settle(&mut app, Message::Reload).await;
        let request = store(&app).pending_incoming_pairings()[0].id;
        settle(
            &mut app,
            Message::AnswerPairing {
                pairing_id: request,
                accept: true,
            },
        )
        .await;
        assert!(
            app.incoming_prompt().is_none(),
            "without waiting for the event"
        );
        assert!(core.device(&peer.device_id).unwrap().paired);
        assert!(app.answering.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_answer_keeps_the_prompt_open_with_the_reason() {
        let mut app = running();
        // A request the core no longer has.
        let request = PairingSnapshot {
            id: Uuid::from_u128(1),
            device_id: testing::PEER_ID.into(),
            device_name: "Peer".into(),
            direction: PairingDirection::Incoming,
            status: PairingStatus::AwaitingConfirmation,
            verification_code: Some("1234ABCD".into()),
            created_at: 0,
            expires_at: 30_000,
            error_code: None,
        };
        let Phase::Running(running) = &mut app.phase else {
            unreachable!()
        };
        running.ctx.store_mut().apply_snapshot(Snapshot {
            devices: Ok(Vec::new()),
            pairings: Ok(vec![request.clone()]),
            transfers: Vec::new(),
            settings: Err(String::new()),
        });

        settle(
            &mut app,
            Message::AnswerPairing {
                pairing_id: request.id,
                accept: true,
            },
        )
        .await;
        assert!(app.incoming_prompt().is_some());
        assert_eq!(
            app.answer_error,
            Some((request.id, "That pairing request no longer exists.".into()))
        );
        assert!(app.answering.is_none());
        // Escape doesn't dismiss it.
        settle(&mut app, Message::Key(KeyCommand::Cancel)).await;
        assert!(app.incoming_prompt().is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn the_transfers_page_shows_progress_and_cancels() {
        let mut app = running();
        let core = core(&app);
        let (peer, _sent) = testing::connect_peer(&core, testing::PEER_ID, &[]);
        let mut transfer =
            core.transfers()
                .begin(&peer, TransferDirection::Incoming, "movie.mkv".into(), 100);
        transfer.transferring();
        transfer.progress(50);
        settle(&mut app, Message::Reload).await;
        settle(
            &mut app,
            Message::Navigate(Route::Transfers, Origin::Window),
        )
        .await;

        let mut ui = Simulator::new(app.view(app.window.unwrap()));
        assert!(ui.find("From Peer · 50 bytes of 100 bytes").is_ok());
        ui.click(widget::Id::from("Cancel")).unwrap();
        let clicked: Vec<_> = ui.into_messages().collect();
        for message in clicked {
            settle(&mut app, message).await;
        }
        assert!(transfer.cancellation().is_cancelled(), "the core was asked");
        assert!(app.toasts.is_empty());

        // The transfer's task notices, and ends it.
        let transfer_id = transfer.id();
        transfer.cancelled();
        settle(&mut app, Message::Reload).await;
        let mut ui = Simulator::new(app.view(app.window.unwrap()));
        assert!(ui.find("From Peer · Cancelled").is_ok());
        assert!(ui.find(widget::Id::from("Cancel")).is_err());
        drop(ui);

        // Too late to cancel: say why.
        settle(&mut app, Message::CancelTransfer(transfer_id)).await;
        assert_eq!(
            app.toasts.items()[0].text,
            error::describe_code("invalid_transfer_state")
        );
    }

    /// Opens nothing: records what it was asked, and fails for
    /// `missing.txt`.
    #[derive(Default)]
    struct FakeOpener {
        opened: Mutex<Vec<(PathBuf, bool)>>,
    }

    impl FakeOpener {
        fn record(&self, path: &std::path::Path, reveal: bool) -> Result<(), String> {
            if path.ends_with("missing.txt") {
                return Err("no such file".into());
            }
            self.opened.lock().unwrap().push((path.into(), reveal));
            Ok(())
        }
    }

    impl Open for FakeOpener {
        fn open(&self, path: &std::path::Path) -> Result<(), String> {
            self.record(path, false)
        }

        fn reveal(&self, path: &std::path::Path) -> Result<(), String> {
            self.record(path, true)
        }
    }

    #[tokio::test(start_paused = true)]
    async fn opening_a_received_file_reports_only_failures() {
        let mut app = running();
        let opener = Arc::new(FakeOpener::default());
        app.desktop.opener = opener.clone();
        let notes = PathBuf::from("/home/me/Downloads/notes.txt");

        settle(&mut app, Message::OpenFile(notes.clone())).await;
        settle(&mut app, Message::RevealFile(notes.clone())).await;
        assert!(app.toasts.is_empty());
        assert_eq!(
            *opener.opened.lock().unwrap(),
            [(notes.clone(), false), (notes, true)]
        );

        settle(
            &mut app,
            Message::OpenFile("/home/me/Downloads/missing.txt".into()),
        )
        .await;
        assert_eq!(
            app.toasts.items()[0].text,
            "Couldn’t open /home/me/Downloads/missing.txt"
        );
    }

    /// Click `target` (a text or widget id) in the window, and settle what
    /// that sends.
    async fn click<S>(app: &mut App, target: S)
    where
        S: iced_test::selector::Selector + Send + fmt::Debug + Clone,
        S::Output: iced_test::selector::Bounded + Clone + Send + Sync + 'static,
    {
        let mut ui = Simulator::new(app.view(app.window.unwrap()));
        ui.click(target.clone())
            .unwrap_or_else(|error| panic!("{target:?}: {error:?}"));
        let clicked: Vec<_> = ui.into_messages().collect();
        for message in clicked {
            settle(app, message).await;
        }
    }

    fn shows(app: &App, text: &str) -> bool {
        Simulator::new(app.view(app.window.unwrap()))
            .find(text)
            .is_ok()
    }

    fn settings(app: &App) -> &SettingsSnapshot {
        store(app).settings().loaded().expect("settings loaded")
    }

    #[tokio::test(start_paused = true)]
    async fn renaming_shows_the_new_name_and_a_bad_one_says_why() {
        let mut app = running();
        settle(&mut app, Message::Reload).await;
        settle(&mut app, Message::Navigate(Route::Settings, Origin::Window)).await;
        click(&mut app, "Device name").await;
        let dialog = app.dialogs.current().expect("the name dialog");
        assert_eq!(dialog.title, "Device name");
        assert_eq!(dialog.value(), "MyConnect");

        settle(
            &mut app,
            Message::Dialog(DialogEvent::Input("Bad.Name".into())),
        )
        .await;
        settle(&mut app, Message::Dialog(DialogEvent::Submit)).await;
        let dialog = app.dialogs.current().expect("the dialog stays open");
        assert!(dialog.error().unwrap().contains("1 to 32 characters"));
        assert!(!dialog.is_busy());
        assert_eq!(settings(&app).device_name, "MyConnect");

        settle(
            &mut app,
            Message::Dialog(DialogEvent::Input("Studio".into())),
        )
        .await;
        click(&mut app, "Save").await;
        assert!(app.dialogs.current().is_none());
        assert_eq!(core(&app).settings().unwrap().device_name, "Studio");
        // Without waiting for the event.
        assert!(shows(&app, "Studio"));

        click(&mut app, widget::Id::from("Back")).await;
        assert!(shows(&app, "This computer: Studio"));
    }

    #[tokio::test(start_paused = true)]
    async fn switches_save_their_setting() {
        let (core, plugin, _commands) = crate::core::testing::handle_with_plugin(
            crate::plugins::clipboard::ClipboardPlugin::new(
                crate::plugins::clipboard::InMemoryClipboard::shared(),
            ),
        );
        let features = Features::new(plugin, Arc::new(BrowsePlugin::default()));
        let mut app = running_with(core.clone(), features, &Fakes::default());
        settle(&mut app, Message::Reload).await;
        settle(&mut app, Message::Navigate(Route::Settings, Origin::Window)).await;
        let sync_enabled = |settings: &SettingsSnapshot| {
            crate::plugins::clipboard::ClipboardSettings::of(settings).sync_enabled
        };
        assert!(sync_enabled(settings(&app)));

        // The feature's own section.
        click(&mut app, "Sync clipboard").await;
        assert!(!sync_enabled(&core.settings().unwrap()));
        settle(&mut app, Message::Reload).await;
        assert!(
            !sync_enabled(settings(&app)),
            "its event updates the switch"
        );

        // The shell's.
        assert!(settings(&app).close_to_tray, "on by default");
        click(&mut app, "Keep running when the window is closed").await;
        assert!(!core.settings().unwrap().close_to_tray);
        assert!(
            !settings(&app).close_to_tray,
            "without waiting for the event"
        );
        assert!(app.toasts.is_empty());
    }

    #[tokio::test]
    async fn the_version_is_shown() {
        let mut app = running();
        settle(&mut app, Message::Reload).await;
        settle(&mut app, Message::Navigate(Route::Settings, Origin::Window)).await;
        assert!(shows(&app, "Version"));
        assert!(shows(&app, "1.2.3 (test)"));
    }

    /// Picks what it is told to, and records where it started (folders) or
    /// its title (files).
    #[derive(Default)]
    struct FakePicker {
        answer: Option<PathBuf>,
        files: Option<Vec<PathBuf>>,
        asked: Mutex<Vec<PathBuf>>,
    }

    impl Pick for FakePicker {
        fn pick_folder(&self, _title: &str, start: &std::path::Path) -> picking::Picked<PathBuf> {
            self.asked.lock().unwrap().push(start.into());
            let answer = self.answer.clone();
            Box::pin(async move { answer })
        }

        fn pick_files(&self, title: &str) -> picking::Picked<Vec<PathBuf>> {
            self.asked.lock().unwrap().push(title.into());
            let files = self.files.clone();
            Box::pin(async move { files })
        }
    }

    fn picker(answer: Option<PathBuf>) -> Arc<FakePicker> {
        Arc::new(FakePicker {
            answer,
            ..FakePicker::default()
        })
    }

    #[tokio::test(start_paused = true)]
    async fn the_download_folder_is_picked_from_the_current_one() {
        let mut app = running();
        settle(&mut app, Message::Reload).await;
        settle(&mut app, Message::Navigate(Route::Settings, Origin::Window)).await;
        let current = settings(&app).download_dir.clone();
        let folder = tempfile::tempdir().unwrap();

        // Cancelled: nothing changes.
        let cancelled = picker(None);
        app.desktop.picker = cancelled.clone();
        click(&mut app, "Save received files in").await;
        assert_eq!(
            *cancelled.asked.lock().unwrap(),
            std::slice::from_ref(&current)
        );
        assert_eq!(settings(&app).download_dir, current);

        let chosen = picker(Some(folder.path().into()));
        app.desktop.picker = chosen.clone();
        click(&mut app, "Save received files in").await;
        assert_eq!(*chosen.asked.lock().unwrap(), [current]);
        assert_eq!(core(&app).settings().unwrap().download_dir, folder.path());
        assert_eq!(settings(&app).download_dir, folder.path());
        assert!(shows(&app, &folder.path().display().to_string()));
        assert!(app.toasts.is_empty());

        // A folder the daemon refuses: say why, and keep the old one.
        app.desktop.picker = picker(Some("relative/folder".into()));
        click(&mut app, "Save received files in").await;
        assert_eq!(
            app.toasts.items()[0].text,
            "That folder can’t be used for downloads."
        );
        assert_eq!(settings(&app).download_dir, folder.path());
    }

    /// An app over a core that runs share, with `photo.jpg` and `notes.txt` to send from a folder that
    /// also holds a folder, `album`.
    struct Sharing {
        app: App,
        core: Core,
        files: tempfile::TempDir,
    }

    impl Sharing {
        fn new() -> Self {
            let (core, _plugin, _commands) =
                crate::core::testing::handle_with_plugin(crate::plugins::share::SharePlugin);
            let app = running_on(core.clone());
            let files = tempfile::tempdir().unwrap();
            std::fs::write(files.path().join("photo.jpg"), "jpg").unwrap();
            std::fs::write(files.path().join("notes.txt"), "txt").unwrap();
            std::fs::create_dir(files.path().join("album")).unwrap();
            Self { app, core, files }
        }

        fn file(&self, name: &str) -> PathBuf {
            self.files.path().join(name)
        }

        /// A paired, connected peer; one that takes files if `capable`.
        async fn peer(&mut self, device_id: &str, capable: bool) -> String {
            let capabilities: &[&str] = if capable {
                &[crate::plugins::share::PACKET_TYPE]
            } else {
                &[]
            };
            let (peer, sent) = testing::connect_peer(&self.core, device_id, capabilities);
            // The connection lasts as long as its receiver.
            std::mem::forget(sent);
            settle(&mut self.app, Message::Reload).await;
            peer.device_id
        }

        /// Drag `paths` over the window and drop them, as winit reports it.
        async fn drop(&mut self, paths: &[PathBuf]) {
            self.hover(paths).await;
            for path in paths {
                self.window(window::Event::FileDropped(path.clone())).await;
            }
        }

        async fn hover(&mut self, paths: &[PathBuf]) {
            for path in paths {
                self.window(window::Event::FileHovered(path.clone())).await;
            }
        }

        async fn window(&mut self, event: window::Event) {
            let id = self.app.window.unwrap();
            settle(&mut self.app, Message::Window(id, event)).await;
        }

        /// To whom each file sent so far went, by file name. (Transfers
        /// started in the same millisecond can't be told apart by age.)
        fn sent(&self) -> Vec<(String, String)> {
            let mut sent: Vec<_> = self
                .core
                .transfers()
                .list()
                .into_iter()
                .map(|transfer| (transfer.device_id, transfer.file_name))
                .collect();
            sent.sort_by(|a, b| a.1.cmp(&b.1));
            sent
        }
    }

    const OTHER_PEER: &str = "8f9e2a1c3b4d4e5f8a9b0c1d2e3f4a5b";

    #[tokio::test(start_paused = true)]
    async fn files_dropped_on_a_device_page_go_straight_to_it() {
        let mut sharing = Sharing::new();
        let peer = sharing.peer(testing::PEER_ID, true).await;
        let _other = sharing.peer(OTHER_PEER, true).await;
        sharing.app.route = Route::Device(peer.clone());
        let (photo, notes) = (sharing.file("photo.jpg"), sharing.file("notes.txt"));

        sharing.hover(&[photo.clone(), notes.clone()]).await;
        assert_eq!(
            sharing.app.drop_hint().as_deref(),
            Some("Drop to send to Peer")
        );
        assert!(shows(&sharing.app, "Drop to send to Peer"));
        for path in [&photo, &notes] {
            sharing
                .window(window::Event::FileDropped(path.clone()))
                .await;
        }

        assert_eq!(
            sharing.sent(),
            [
                (peer.clone(), "notes.txt".into()),
                (peer.clone(), "photo.jpg".into())
            ]
        );
        assert!(sharing.app.choosing.is_none());
        assert_eq!(sharing.app.route, Route::Device(peer));
        assert!(sharing.app.drop_hint().is_none(), "the hint goes");
        assert!(sharing.app.toasts.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn files_dropped_on_a_browse_folder_upload_there_and_elsewhere_send() {
        use crate::ui::features::browse::tests::{Call, phone_files};

        let mut sharing = Sharing::new();
        let phone = phone_files();
        features_mut(&mut sharing.app).browse = browse::BrowseUi::with_files(phone.clone());
        let (peer, sent) = testing::connect_peer(
            &sharing.core,
            testing::PEER_ID,
            &[
                crate::plugins::share::PACKET_TYPE,
                crate::plugins::browse::REQUEST_PACKET_TYPE,
            ],
        );
        std::mem::forget(sent);
        settle(&mut sharing.app, Message::Reload).await;
        let peer = peer.device_id;

        // The device page's action opens the storage.
        settle(
            &mut sharing.app,
            Message::Navigate(Route::Device(peer.clone()), Origin::Window),
        )
        .await;
        click(&mut sharing.app, "Browse files").await;
        assert_eq!(
            sharing.app.route,
            Route::Browse {
                device: peer.clone(),
                folder: None
            }
        );
        assert!(shows(&sharing.app, "Files on Peer"));
        click(&mut sharing.app, "All files").await;
        let folder = "/storage/emulated/0";
        assert_eq!(
            sharing.app.route,
            Route::Browse {
                device: peer.clone(),
                folder: Some(folder.into())
            }
        );

        let photo = sharing.file("photo.jpg");
        sharing.hover(std::slice::from_ref(&photo)).await;
        assert_eq!(
            sharing.app.drop_hint().as_deref(),
            Some("Drop to upload to All files")
        );
        sharing
            .window(window::Event::FileDropped(photo.clone()))
            .await;
        assert!(phone.calls().contains(&Call::Upload {
            folder: folder.into(),
            local: photo.clone(),
        }));
        assert!(sharing.sent().is_empty(), "uploaded, not sent");
        assert!(sharing.app.choosing.is_none());

        // The storage isn't a folder: a drop there sends to the device.
        click(&mut sharing.app, "Storage").await;
        sharing.drop(std::slice::from_ref(&photo)).await;
        assert_eq!(sharing.sent(), [(peer.clone(), "photo.jpg".into())]);
    }

    #[tokio::test]
    async fn a_browse_page_of_a_forgotten_device_says_so() {
        let mut app = running();
        app.route = Route::Browse {
            device: "gone".into(),
            folder: None,
        };
        assert!(shows(&app, "This device is no longer known."));
    }

    #[tokio::test(start_paused = true)]
    async fn files_dropped_away_from_a_device_ask_where_to_go() {
        let mut sharing = Sharing::new();
        let peer = sharing.peer(testing::PEER_ID, true).await;
        sharing.app.route = Route::Transfers;
        let photo = sharing.file("photo.jpg");

        sharing.hover(std::slice::from_ref(&photo)).await;
        assert_eq!(
            sharing.app.drop_hint().as_deref(),
            Some(dropping::CHOOSE_LABEL)
        );
        sharing
            .window(window::Event::FileDropped(photo.clone()))
            .await;
        assert!(shows(&sharing.app, "Send photo.jpg"));
        assert!(sharing.sent().is_empty());

        click(&mut sharing.app, "Peer").await;
        assert!(sharing.app.choosing.is_none());
        assert_eq!(sharing.sent(), [(peer.clone(), "photo.jpg".into())]);
        // The device's page, where the transfer shows.
        assert_eq!(sharing.app.route, Route::Device(peer));
    }

    #[tokio::test(start_paused = true)]
    async fn a_drop_on_a_device_that_cant_take_files_asks_instead() {
        let mut sharing = Sharing::new();
        let incapable = sharing.peer(testing::PEER_ID, false).await;
        let capable = sharing.peer(OTHER_PEER, true).await;
        sharing.app.route = Route::Device(incapable);

        sharing.hover(&[sharing.file("photo.jpg")]).await;
        assert_eq!(
            sharing.app.drop_hint().as_deref(),
            Some(dropping::CHOOSE_LABEL)
        );
        sharing.drop(&[sharing.file("photo.jpg")]).await;
        let recipients: Vec<_> = sharing
            .app
            .recipients()
            .iter()
            .map(|device| device.device_id.clone())
            .collect();
        assert_eq!(recipients, [capable]);

        // Escape closes it, as Cancel does.
        settle(&mut sharing.app, Message::Key(KeyCommand::Cancel)).await;
        assert!(sharing.app.choosing.is_none());
        assert!(sharing.sent().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn the_chooser_follows_devices_and_says_when_none_can_take_files() {
        let mut sharing = Sharing::new();
        sharing.drop(&[sharing.file("photo.jpg")]).await;
        assert!(shows(
            &sharing.app,
            "No paired device is connected and able to receive files."
        ));

        let peer = sharing.peer(testing::PEER_ID, true).await;
        assert_eq!(sharing.app.recipients()[0].device_id, peer);
        assert!(shows(&sharing.app, "Choose the device to send to:"));

        click(&mut sharing.app, "Cancel").await;
        assert!(sharing.app.choosing.is_none());
        assert!(sharing.sent().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn a_dropped_folder_is_refused() {
        let mut sharing = Sharing::new();
        let peer = sharing.peer(testing::PEER_ID, true).await;
        sharing.app.route = Route::Device(peer.clone());

        sharing.drop(&[sharing.file("album")]).await;
        assert_eq!(
            sharing.app.toasts.items()[0].text,
            "Only files can be sent, not folders."
        );
        assert!(sharing.sent().is_empty());

        // Among files, a folder is left out.
        sharing
            .drop(&[sharing.file("album"), sharing.file("notes.txt")])
            .await;
        assert_eq!(sharing.sent(), [(peer, "notes.txt".into())]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_drop_without_hover_events_still_arrives_whole() {
        let mut sharing = Sharing::new();
        let peer = sharing.peer(testing::PEER_ID, true).await;
        sharing.app.route = Route::Device(peer.clone());
        let (photo, notes) = (sharing.file("photo.jpg"), sharing.file("notes.txt"));

        let id = sharing.app.window.unwrap();
        let first = step(
            &mut sharing.app,
            Message::Window(id, window::Event::FileDropped(photo)),
        )
        .await;
        sharing.window(window::Event::FileDropped(notes)).await;
        assert!(sharing.sent().is_empty(), "waits for the rest");
        for message in first {
            settle(&mut sharing.app, message).await;
        }
        assert_eq!(
            sharing.sent(),
            [
                (peer.clone(), "notes.txt".into()),
                (peer, "photo.jpg".into())
            ]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn drops_are_ignored_while_the_pairing_prompt_shows() {
        let mut sharing = Sharing::new();
        let peer = sharing.peer(testing::PEER_ID, true).await;
        let (stranger, _sent) = testing::connect_unpaired_peer(&sharing.core, OTHER_PEER);
        testing::request_pairing(&sharing.core, &stranger.device_id);
        settle(&mut sharing.app, Message::Reload).await;
        sharing.app.route = Route::Device(peer);

        sharing.hover(&[sharing.file("photo.jpg")]).await;
        assert!(sharing.app.drop_hint().is_none());
        sharing.drop(&[sharing.file("photo.jpg")]).await;
        assert!(sharing.sent().is_empty());
        assert!(sharing.app.choosing.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn send_files_picks_files_and_sends_them() {
        let mut sharing = Sharing::new();
        let peer = sharing.peer(testing::PEER_ID, true).await;
        sharing.app.route = Route::Device(peer.clone());

        // Cancelled: nothing is sent.
        let cancelled = picker(None);
        sharing.app.desktop.picker = cancelled.clone();
        click(&mut sharing.app, "Send files").await;
        assert_eq!(
            *cancelled.asked.lock().unwrap(),
            [PathBuf::from("Send files to Peer")]
        );
        assert!(sharing.sent().is_empty());

        sharing.app.desktop.picker = Arc::new(FakePicker {
            files: Some(vec![sharing.file("photo.jpg"), sharing.file("gone.txt")]),
            ..FakePicker::default()
        });
        click(&mut sharing.app, "Send files").await;
        assert_eq!(sharing.sent(), [(peer, "photo.jpg".into())]);
        assert_eq!(
            sharing.app.toasts.items()[0].text,
            "Couldn’t send gone.txt: The file couldn’t be read."
        );
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
                version: "1.2.3 (test)".into(),
                data_dir: None,
            },
            start,
            Service::default(),
            Fakes::default().desktop(),
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

    // The tray, closing, quitting and notifications: Flutter's
    // `background_host_test.dart`.

    /// An app on `fakes`.
    fn background(fakes: &Fakes) -> App {
        let (core, _commands) = handle();
        running_on_desktop(core, fakes)
    }

    /// A paired peer, connected and taking `capabilities`, as the app
    /// sees it. The connection lasts as long as the receiver.
    async fn peer(
        app: &mut App,
        capabilities: &[&str],
    ) -> tokio::sync::mpsc::Receiver<crate::protocol::Packet> {
        let (_, sent) = testing::connect_peer(&core(app), testing::PEER_ID, capabilities);
        settle(app, Message::Reload).await;
        sent
    }

    /// The menu's top level: labels, and "-" for separators.
    fn tray_labels(fakes: &Fakes) -> Vec<String> {
        fakes
            .menu()
            .iter()
            .map(|item| match item {
                TrayItem::Item { label, .. } | TrayItem::Submenu { label, .. } => label.clone(),
                TrayItem::Separator => "-".into(),
            })
            .collect()
    }

    /// The item at `path` in the tray menu, by labels.
    fn tray_item(fakes: &Fakes, path: &[&str]) -> Option<TrayItem> {
        let mut items = fakes.menu();
        let (last, parents) = path.split_last()?;
        for label in parents {
            items = items.into_iter().find_map(|item| match item {
                TrayItem::Submenu {
                    label: found,
                    items,
                } if found == *label => Some(items),
                _ => None,
            })?;
        }
        items.into_iter().find(|item| match item {
            TrayItem::Item { label, .. } | TrayItem::Submenu { label, .. } => label == last,
            TrayItem::Separator => false,
        })
    }

    fn tray_enabled(fakes: &Fakes, path: &[&str]) -> bool {
        match tray_item(fakes, path) {
            Some(TrayItem::Item { command, .. }) => command.is_some(),
            Some(TrayItem::Submenu { .. }) => true,
            _ => panic!("no tray item {path:?}"),
        }
    }

    /// Choose the tray item at `path`.
    async fn choose(app: &mut App, fakes: &Fakes, path: &[&str]) {
        let Some(TrayItem::Item {
            command: Some(command),
            ..
        }) = tray_item(fakes, path)
        else {
            panic!("no enabled tray item {path:?}");
        };
        settle(app, Message::Desktop(DesktopEvent::TrayChose(command))).await;
    }

    /// Press the window's close button.
    async fn close(app: &mut App) {
        let id = app.window.expect("the window is open");
        settle(app, Message::Window(id, window::Event::CloseRequested)).await;
    }

    fn placements(dir: &tempfile::TempDir) -> PlacementStore {
        PlacementStore::new(dir.path().join("window.json"))
    }

    fn bounds(x: f32, y: f32, width: f32, height: f32) -> Option<iced::Rectangle> {
        Some(iced::Rectangle {
            x,
            y,
            width,
            height,
        })
    }

    #[tokio::test(start_paused = true)]
    async fn closing_the_window_keeps_the_app_in_the_tray_and_the_tray_shows_it() {
        let dir = tempfile::tempdir().unwrap();
        let fakes = Fakes {
            placements: Some(placements(&dir)),
            ..Fakes::default()
        };
        let mut app = background(&fakes);
        settle(&mut app, Message::Reload).await;

        close(&mut app).await;
        assert!(app.window.is_none());
        assert!(!app.quitting, "the daemon keeps running");
        assert_eq!(
            placements(&dir).load(),
            Some(Placement {
                visible: false,
                maximized: false,
                bounds: bounds(30.0, 40.0, 500.0, 700.0),
            })
        );

        settle(&mut app, Message::Desktop(DesktopEvent::TrayClicked)).await;
        assert!(app.window.is_some());
        let opened = fakes.windows.opened.lock().unwrap();
        let reopened = opened.last().unwrap();
        assert!(
            matches!(
                reopened.position,
                window::Position::Specific(iced::Point { x: 30.0, y: 40.0 })
            ),
            "where it was"
        );
        assert_eq!(reopened.size, iced::Size::new(500.0, 700.0));
        assert!(placements(&dir).load().unwrap().visible);
    }

    #[tokio::test(start_paused = true)]
    async fn closing_the_window_quits_when_close_to_tray_is_off() {
        let dir = tempfile::tempdir().unwrap();
        let fakes = Fakes {
            placements: Some(placements(&dir)),
            ..Fakes::default()
        };
        let mut app = background(&fakes);
        core(&app)
            .update_settings(SettingsPatch {
                close_to_tray: Some(Some(false)),
                ..SettingsPatch::default()
            })
            .unwrap();
        settle(&mut app, Message::Reload).await;

        close(&mut app).await;
        assert!(app.quitting);
        // The next launch shows it again, where it was.
        assert_eq!(
            placements(&dir).load(),
            Some(Placement {
                visible: true,
                maximized: false,
                bounds: bounds(30.0, 40.0, 500.0, 700.0),
            })
        );
    }

    #[tokio::test(start_paused = true)]
    async fn without_a_tray_closing_quits_and_the_window_always_shows() {
        let dir = tempfile::tempdir().unwrap();
        placements(&dir).save(Placement {
            visible: false,
            ..Placement::default()
        });
        let fakes = Fakes {
            placements: Some(placements(&dir)),
            no_tray: true,
            ..Fakes::default()
        };
        let mut app = background(&fakes);
        settle(&mut app, Message::Reload).await;
        assert!(app.window.is_some(), "shown, though it was hidden");

        close(&mut app).await;
        assert!(app.quitting);
    }

    #[tokio::test(start_paused = true)]
    async fn a_tray_host_going_away_brings_the_window_back() {
        let fakes = Fakes::default();
        let mut app = background(&fakes);
        settle(&mut app, Message::Reload).await;
        close(&mut app).await;
        assert!(app.window.is_none());

        settle(
            &mut app,
            Message::Desktop(DesktopEvent::TrayAvailable(false)),
        )
        .await;
        assert!(app.window.is_some());
        close(&mut app).await;
        assert!(app.quitting, "nothing could show it again");
    }

    #[tokio::test(start_paused = true)]
    async fn quitting_from_the_tray_saves_the_window_and_starts_hidden_next_time() {
        let dir = tempfile::tempdir().unwrap();
        let fakes = Fakes {
            placements: Some(placements(&dir)),
            ..Fakes::default()
        };
        let mut app = background(&fakes);
        settle(&mut app, Message::Reload).await;
        close(&mut app).await;

        choose(&mut app, &fakes, &["Quit"]).await;
        assert!(app.quitting);
        assert!(!placements(&dir).load().unwrap().visible);
        // Asked again, by a signal: nothing more happens.
        assert!(
            step(&mut app, Message::Desktop(DesktopEvent::QuitRequested))
                .await
                .is_empty()
        );

        let next = app_on(tokio::runtime::Handle::current(), &fakes);
        assert!(next.window.is_none(), "it starts in the tray");
    }

    #[tokio::test(start_paused = true)]
    async fn the_window_placement_is_saved_once_it_stays_put() {
        let dir = tempfile::tempdir().unwrap();
        let fakes = Fakes {
            placements: Some(placements(&dir)),
            ..Fakes::default()
        };
        let mut app = background(&fakes);
        let id = app.window.unwrap();
        let moved = step(
            &mut app,
            Message::Window(id, window::Event::Moved(iced::Point::new(1.0, 2.0))),
        )
        .await;
        let resized = step(
            &mut app,
            Message::Window(id, window::Event::Resized(iced::Size::new(3.0, 4.0))),
        )
        .await;
        // Only the last change reads the window.
        let [Message::SavePlacement(first)] = moved[..] else {
            panic!("unexpected messages: {moved:?}");
        };
        assert!(
            step(&mut app, Message::SavePlacement(first))
                .await
                .is_empty()
        );
        let [Message::SavePlacement(last)] = resized[..] else {
            panic!("unexpected messages: {resized:?}");
        };
        settle(&mut app, Message::SavePlacement(last)).await;
        assert_eq!(
            placements(&dir).load().unwrap().bounds,
            bounds(30.0, 40.0, 500.0, 700.0)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_pairing_request_notifies_while_the_window_is_closed() {
        let fakes = Fakes::default();
        let mut app = background(&fakes);
        settle(&mut app, Message::Reload).await;
        close(&mut app).await;

        let core = core(&app);
        let (_, _sent) = testing::connect_unpaired_peer(&core, testing::PEER_ID);
        testing::request_pairing(&core, testing::PEER_ID);
        settle(&mut app, Message::Reload).await;
        assert_eq!(fakes.notified(), ["Peer wants to pair with this computer."]);

        settle(
            &mut app,
            Message::Desktop(DesktopEvent::NotificationClicked),
        )
        .await;
        assert!(app.window.is_some());
        assert!(shows(&app, "Pairing request"));

        // Resolved elsewhere: the notification goes with the prompt.
        let pairing = store(&app).pending_incoming_pairings()[0].id;
        core.cancel_pairing(pairing).unwrap();
        settle(&mut app, Message::Reload).await;
        assert!(fakes.notified().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn a_pairing_request_does_not_notify_over_a_focused_window() {
        let fakes = Fakes::default();
        let mut app = background(&fakes);
        settle(&mut app, Message::Reload).await;
        let core = core(&app);
        let (_, _first) = testing::connect_unpaired_peer(&core, testing::PEER_ID);
        testing::request_pairing(&core, testing::PEER_ID);
        settle(&mut app, Message::Reload).await;
        assert!(shows(&app, "Pairing request"));
        assert!(fakes.notified().is_empty());

        // Losing focus later doesn't bring up a stale notification.
        let id = app.window.unwrap();
        settle(&mut app, Message::Window(id, window::Event::Unfocused)).await;
        let other = "0123456789abcdef0123456789abcdef";
        let (_, _second) = testing::connect_unpaired_peer(&core, other);
        testing::request_pairing(&core, other);
        settle(&mut app, Message::Reload).await;
        assert_eq!(fakes.notified().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_received_file_notifies_while_the_window_is_closed() {
        let fakes = Fakes::default();
        let mut app = background(&fakes);
        let core = core(&app);
        let (peer, _sent) = testing::connect_peer(&core, testing::PEER_ID, &[]);
        // Transferring, since only a transfer under way can complete.
        let begin = |direction, name: &str| {
            let transfer = core.transfers().begin(&peer, direction, name.into(), 1);
            transfer.transferring();
            transfer
        };
        begin(TransferDirection::Incoming, "earlier.jpg").complete(None);
        let incoming = begin(TransferDirection::Incoming, "photo.jpg");
        let outgoing = begin(TransferDirection::Outgoing, "sent.jpg");
        settle(&mut app, Message::Reload).await;
        close(&mut app).await;

        incoming.complete(None);
        outgoing.complete(None);
        settle(&mut app, Message::Reload).await;
        assert_eq!(fakes.notified(), ["photo.jpg from Peer"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_ping_notifies_while_closed_and_toasts_otherwise() {
        let fakes = Fakes::default();
        let mut app = background(&fakes);
        let ping = |message: Option<&str>| {
            Message::Sync(sync::Update::Event(Box::new(crate::core::CoreEvent {
                sequence: 1,
                timestamp: 0,
                event: crate::core::EventData::Plugin(
                    crate::core::PluginEvent::new(&crate::plugins::ping::ReceivedPing {
                        device_id: "pixel".into(),
                        device_name: "Pixel".into(),
                        message: message.map(Into::into),
                    })
                    .unwrap(),
                ),
            })))
        };
        settle(&mut app, ping(Some("hi"))).await;
        assert_eq!(app.toasts.items()[0].text, "Pixel: hi");
        assert!(fakes.notified().is_empty());

        close(&mut app).await;
        settle(&mut app, ping(None)).await;
        assert_eq!(fakes.notified(), ["Ping!"]);
    }

    #[tokio::test(start_paused = true)]
    async fn the_tray_lists_connected_paired_devices_then_settings_and_quit() {
        let fakes = Fakes::default();
        let (core, _plugin, _commands) = crate::core::testing::handle_with_plugin(
            crate::plugins::battery::BatteryPlugin::default(),
        );
        let mut app = running_on_desktop(core.clone(), &fakes);
        // Paired, but away; and connected, but not paired.
        core.discover_device(
            &crate::core::testing::make_identity("0123456789abcdef0123456789abcdef", vec![]),
            true,
            1,
        )
        .unwrap();
        let (_, _unpaired) =
            testing::connect_unpaired_peer(&core, "fedcba9876543210fedcba9876543210");
        let _sent = peer(
            &mut app,
            &[
                crate::plugins::ping::PACKET_TYPE,
                crate::plugins::findmyphone::REQUEST_PACKET_TYPE,
            ],
        )
        .await;
        // The demo phone's first report: 82%.
        for packet in crate::ui::features::battery::demo_packets(&testing::device("Phone"), 0) {
            core.handle_peer_packet(testing::PEER_ID, packet);
        }
        settle(&mut app, Message::Reload).await;

        assert_eq!(
            tray_labels(&fakes),
            [
                "Open MyConnect",
                "-",
                "Peer · 82%",
                "-",
                "Settings",
                "-",
                "Quit"
            ]
        );
        assert!(tray_enabled(&fakes, &["Peer · 82%", "Ping"]));
        assert!(tray_enabled(&fakes, &["Peer · 82%", "Ring"]));
        assert!(tray_enabled(&fakes, &["Peer · 82%", "Show details"]));
    }

    #[tokio::test(start_paused = true)]
    async fn the_tray_lists_ring_only_for_a_device_that_can() {
        let fakes = Fakes::default();
        let mut app = background(&fakes);
        let _sent = peer(&mut app, &[crate::plugins::ping::PACKET_TYPE]).await;
        assert!(tray_enabled(&fakes, &["Peer", "Ping"]));
        assert!(tray_item(&fakes, &["Peer", "Ring"]).is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn the_tray_says_when_nothing_is_paired_or_connected() {
        let fakes = Fakes::default();
        let mut app = background(&fakes);
        assert_eq!(
            tray_labels(&fakes),
            ["Open MyConnect", "-", "Settings", "-", "Quit"],
            "devices unknown yet"
        );
        settle(&mut app, Message::Reload).await;
        assert!(!tray_enabled(&fakes, &["No paired devices"]));

        core(&app)
            .discover_device(
                &crate::core::testing::make_identity(testing::PEER_ID, vec![]),
                true,
                1,
            )
            .unwrap();
        settle(&mut app, Message::Reload).await;
        assert!(!tray_enabled(&fakes, &["No devices connected"]));
        assert!(tray_item(&fakes, &["Peer"]).is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn the_tray_menu_is_sent_only_when_it_changes() {
        let fakes = Fakes::default();
        let mut app = background(&fakes);
        let _sent = peer(&mut app, &[]).await;
        let sent = fakes.tray.updates.load(Ordering::SeqCst);
        settle(&mut app, Message::Reload).await;
        settle(
            &mut app,
            Message::Navigate(Route::Transfers, Origin::Window),
        )
        .await;
        assert_eq!(fakes.tray.updates.load(Ordering::SeqCst), sent);
    }

    #[tokio::test(start_paused = true)]
    async fn the_tray_opens_a_device_or_settings_in_the_window() {
        let fakes = Fakes::default();
        let mut app = background(&fakes);
        let _sent = peer(&mut app, &[]).await;
        close(&mut app).await;

        choose(&mut app, &fakes, &["Peer", "Show details"]).await;
        assert!(app.window.is_some());
        assert_eq!(app.route, Route::Device(testing::PEER_ID.into()));

        choose(&mut app, &fakes, &["Settings"]).await;
        assert_eq!(app.route, Route::Settings);
        assert_eq!(
            fakes.windows.raised.load(Ordering::SeqCst),
            1,
            "already open: raised"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_tray_action_that_opens_a_page_shows_the_window() {
        let fakes = Fakes::default();
        let mut app = background(&fakes);
        let _sent = peer(&mut app, &[crate::plugins::browse::REQUEST_PACKET_TYPE]).await;
        close(&mut app).await;

        choose(&mut app, &fakes, &["Peer", "Browse files"]).await;
        assert!(app.window.is_some());
        assert_eq!(
            app.route,
            Route::Browse {
                device: testing::PEER_ID.into(),
                folder: None
            }
        );

        // A toast is for the window, not the tray.
        settle(
            &mut app,
            Message::Toast {
                text: "Opening".into(),
                action: None,
                origin: Origin::Tray,
            },
        )
        .await;
        assert!(app.toasts.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn pinging_from_the_tray_reports_only_a_failure() {
        let fakes = Fakes::default();
        let mut app = background(&fakes);
        let mut sent = peer(&mut app, &[crate::plugins::ping::PACKET_TYPE]).await;
        close(&mut app).await;

        choose(&mut app, &fakes, &["Peer", "Ping"]).await;
        assert_eq!(
            sent.try_recv().unwrap().packet_type,
            crate::plugins::ping::PACKET_TYPE
        );
        assert!(app.window.is_none());
        assert!(fakes.notified().is_empty());

        // Chosen from a menu that is out of date.
        let Some(TrayItem::Item {
            command: Some(ping),
            ..
        }) = tray_item(&fakes, &["Peer", "Ping"])
        else {
            panic!("a ping item");
        };
        core(&app).forget_device(testing::PEER_ID).unwrap();
        settle(&mut app, Message::Desktop(DesktopEvent::TrayChose(ping))).await;
        assert!(app.window.is_none());
        assert_eq!(
            *fakes
                .notifier
                .shown
                .lock()
                .unwrap()
                .values()
                .next()
                .unwrap(),
            (
                "Couldn’t ping Peer".into(),
                "That device is no longer known.".into()
            )
        );
    }

    #[tokio::test(start_paused = true)]
    async fn sending_files_from_the_tray_rechecks_the_device_after_the_picker() {
        let fakes = Fakes::default();
        let (core, _plugin, _commands) =
            crate::core::testing::handle_with_plugin(crate::plugins::share::SharePlugin);
        let mut app = running_on_desktop(core.clone(), &fakes);
        let files = tempfile::tempdir().unwrap();
        let photo = files.path().join("photo.jpg");
        std::fs::write(&photo, "jpg").unwrap();
        let _sent = peer(&mut app, &[crate::plugins::share::PACKET_TYPE]).await;
        close(&mut app).await;

        app.desktop.picker = Arc::new(FakePicker {
            files: Some(vec![photo.clone()]),
            ..FakePicker::default()
        });
        choose(&mut app, &fakes, &["Peer", "Send files"]).await;
        assert_eq!(core.transfers().list().len(), 1, "sent");
        assert!(app.window.is_none(), "the window stays closed");
        assert!(fakes.notified().is_empty());

        // The device goes while the picker is open.
        let Some(TrayItem::Item {
            command: Some(send),
            ..
        }) = tray_item(&fakes, &["Peer", "Send files"])
        else {
            panic!("a send item");
        };
        core.forget_device(testing::PEER_ID).unwrap();
        settle(&mut app, Message::Desktop(DesktopEvent::TrayChose(send))).await;
        assert_eq!(
            *fakes
                .notifier
                .shown
                .lock()
                .unwrap()
                .values()
                .next()
                .unwrap(),
            (
                "Couldn’t send to Peer".into(),
                "The device is not connected right now.".into()
            )
        );
    }

    #[tokio::test(start_paused = true)]
    async fn the_tray_works_when_the_daemon_failed_to_start() {
        let fakes = Fakes::default();
        let mut app = app_on(tokio::runtime::Handle::current(), &fakes);
        let _ = app.update(Message::Started(Err("no".into())));
        assert_eq!(
            tray_labels(&fakes),
            ["Open MyConnect", "-", "Settings", "-", "Quit"]
        );
        // No settings to ask: closing keeps the tray as the way out.
        close(&mut app).await;
        assert!(app.window.is_none());
        assert!(!app.quitting);
        choose(&mut app, &fakes, &["Open MyConnect"]).await;
        assert!(app.window.is_some());
    }
}
