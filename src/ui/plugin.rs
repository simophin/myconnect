//! The seam between the UI core and each feature's UI half, which lives in
//! `src/plugins/<name>/ui.rs` and is listed in
//! `plugins::builtin_with_ui`. The UI core never names a feature.
//!
//! A feature implements [`UiPlugin`] with its own typed messages. The shell
//! stores it as a [`ErasedUiPlugin`], which every `UiPlugin` is, and sees
//! its messages only as [`PluginMessage`]s, routed back to it by id.

use std::{any::Any, fmt, future::Future, path::PathBuf, sync::Arc};

use iced::{Element, Subscription, Task, widget::Text};

use crate::{
    core::{Core, CoreEvent, DeviceSnapshot, PluginContext, SettingsSnapshot, TransferSnapshot},
    protocol::Packet,
    ui::route::Route,
};

/// A feature's UI half. Lives in `src/plugins/<name>/ui.rs`.
///
/// Every slot has a default that fills nothing, so a feature implements
/// only the ones it uses.
pub trait UiPlugin: Send + 'static {
    type Message: Clone + fmt::Debug + Send + Sync + 'static;

    /// The same id as the core plugin ("ping").
    fn id(&self) -> &'static str;

    /// A chip on the device card, the detail header and the tray label.
    fn device_status(&self, _device: &DeviceSnapshot) -> Option<DeviceStatus> {
        None
    }

    /// Actions for one device, as data. The detail page draws them as
    /// buttons and the tray as menu items, so the two can't drift apart.
    fn device_actions(&self, _device: &DeviceSnapshot) -> Vec<DeviceAction<Self::Message>> {
        Vec::new()
    }

    /// The page named `page` of this plugin, for `device`
    /// ([`Route::Plugin`]).
    fn view_page<'a>(
        &'a self,
        _ctx: &UiContext,
        _device: &'a DeviceSnapshot,
        _page: &str,
    ) -> Option<Element<'a, Self::Message>> {
        None
    }

    /// What to do with files dropped on `device` while the window shows
    /// `route`, if this plugin takes them.
    fn drop_target(
        &self,
        _device: &DeviceSnapshot,
        _route: &Route,
    ) -> Option<DropTarget<Self::Message>> {
        None
    }

    /// A section of the settings page.
    fn view_settings<'a>(
        &'a self,
        _settings: &'a SettingsSnapshot,
    ) -> Option<Element<'a, Self::Message>> {
        None
    }

    /// Every core event, the plugin's own (`ping.received`) and the rest.
    fn on_event(&mut self, _ctx: &UiContext, _event: &CoreEvent) -> Command<Self::Message> {
        Command::none()
    }

    fn update(&mut self, ctx: &UiContext, message: Self::Message) -> Command<Self::Message>;

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }

    /// `--demo`: the packets `device`, one of the made-up connected devices,
    /// sends at step `tick` (every few seconds, from 0), as a peer running
    /// this feature would.
    fn demo_packets(&self, _device: &DeviceSnapshot, _tick: u64) -> Vec<Packet> {
        Vec::new()
    }
}

/// A [`UiPlugin`] as the shell stores it: its messages are
/// [`PluginMessage`]s. Implemented for every `UiPlugin`; don't implement it
/// yourself.
pub trait ErasedUiPlugin: Send {
    fn id(&self) -> &'static str;
    fn device_status(&self, device: &DeviceSnapshot) -> Option<DeviceStatus>;
    fn device_actions(&self, device: &DeviceSnapshot) -> Vec<DeviceAction<PluginMessage>>;
    fn view_page<'a>(
        &'a self,
        ctx: &UiContext,
        device: &'a DeviceSnapshot,
        page: &str,
    ) -> Option<Element<'a, PluginMessage>>;
    fn drop_target(
        &self,
        device: &DeviceSnapshot,
        route: &Route,
    ) -> Option<DropTarget<PluginMessage>>;
    fn view_settings<'a>(
        &'a self,
        settings: &'a SettingsSnapshot,
    ) -> Option<Element<'a, PluginMessage>>;
    fn on_event(&mut self, ctx: &UiContext, event: &CoreEvent) -> Command<PluginMessage>;
    /// Handle a message this plugin produced. Panics if it came from
    /// another plugin: the shell routes by [`PluginMessage::plugin`].
    fn update(&mut self, ctx: &UiContext, message: PluginMessage) -> Command<PluginMessage>;
    fn subscription(&self) -> Subscription<PluginMessage>;
    fn demo_packets(&self, device: &DeviceSnapshot, tick: u64) -> Vec<Packet>;
}

impl<T: UiPlugin> ErasedUiPlugin for T {
    fn id(&self) -> &'static str {
        UiPlugin::id(self)
    }

    fn device_status(&self, device: &DeviceSnapshot) -> Option<DeviceStatus> {
        UiPlugin::device_status(self, device)
    }

    fn device_actions(&self, device: &DeviceSnapshot) -> Vec<DeviceAction<PluginMessage>> {
        let id = UiPlugin::id(self);
        UiPlugin::device_actions(self, device)
            .into_iter()
            .map(|action| action.map(|message| PluginMessage::new(id, message)))
            .collect()
    }

    fn view_page<'a>(
        &'a self,
        ctx: &UiContext,
        device: &'a DeviceSnapshot,
        page: &str,
    ) -> Option<Element<'a, PluginMessage>> {
        let id = UiPlugin::id(self);
        UiPlugin::view_page(self, ctx, device, page)
            .map(|element| element.map(move |message| PluginMessage::new(id, message)))
    }

    fn drop_target(
        &self,
        device: &DeviceSnapshot,
        route: &Route,
    ) -> Option<DropTarget<PluginMessage>> {
        let id = UiPlugin::id(self);
        UiPlugin::drop_target(self, device, route)
            .map(|target| target.map(move |message| PluginMessage::new(id, message)))
    }

    fn view_settings<'a>(
        &'a self,
        settings: &'a SettingsSnapshot,
    ) -> Option<Element<'a, PluginMessage>> {
        let id = UiPlugin::id(self);
        UiPlugin::view_settings(self, settings)
            .map(|element| element.map(move |message| PluginMessage::new(id, message)))
    }

    fn on_event(&mut self, ctx: &UiContext, event: &CoreEvent) -> Command<PluginMessage> {
        let id = UiPlugin::id(self);
        UiPlugin::on_event(self, ctx, event).map(move |message| PluginMessage::new(id, message))
    }

    fn update(&mut self, ctx: &UiContext, message: PluginMessage) -> Command<PluginMessage> {
        let id = UiPlugin::id(self);
        assert_eq!(message.plugin, id, "message routed to the wrong plugin");
        let message = message
            .downcast::<T::Message>()
            .expect("a plugin's messages have its message type");
        UiPlugin::update(self, ctx, message).map(move |message| PluginMessage::new(id, message))
    }

    fn subscription(&self) -> Subscription<PluginMessage> {
        // `Subscription::map` takes only non-capturing closures, so the id
        // travels with each message.
        UiPlugin::subscription(self)
            .with(UiPlugin::id(self))
            .map(|(id, message)| PluginMessage::new(id, message))
    }

    fn demo_packets(&self, device: &DeviceSnapshot, tick: u64) -> Vec<Packet> {
        UiPlugin::demo_packets(self, device, tick)
    }
}

/// A plugin's message, with the plugin's type erased, and the id of the
/// plugin it goes back to.
#[derive(Clone)]
pub struct PluginMessage {
    plugin: &'static str,
    message: Arc<dyn AnyMessage>,
}

impl PluginMessage {
    pub(crate) fn new<M: Any + fmt::Debug + Send + Sync>(plugin: &'static str, message: M) -> Self {
        Self {
            plugin,
            message: Arc::new(message),
        }
    }

    /// The id of the plugin this message belongs to.
    pub fn plugin(&self) -> &'static str {
        self.plugin
    }

    fn downcast<M: Any + Clone>(&self) -> Option<M> {
        // Through `&dyn`, not the `Arc`, which is itself an `AnyMessage`.
        let message: &dyn AnyMessage = &*self.message;
        message.as_any().downcast_ref::<M>().cloned()
    }
}

impl fmt::Debug for PluginMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PluginMessage")
            .field("plugin", &self.plugin)
            .field("message", &self.message)
            .finish()
    }
}

trait AnyMessage: Any + fmt::Debug + Send + Sync {
    fn as_any(&self) -> &dyn Any;
}

impl<T: Any + fmt::Debug + Send + Sync> AnyMessage for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A Lucide icon, as a function so it can be stored as data (the tray
/// can't draw an [`Element`]).
pub type Icon = fn() -> Text<'static>;

/// A plugin's chip on a device: an icon and a short label ("82%").
#[derive(Debug, Clone)]
pub struct DeviceStatus {
    pub icon: Icon,
    pub label: String,
}

/// Something a plugin can do with a device: a button on the device page
/// and an item in the device's tray menu.
#[derive(Debug, Clone)]
pub struct DeviceAction<M> {
    /// Stable within the plugin, for tests and the tray.
    pub id: &'static str,
    pub label: String,
    pub icon: Icon,
    pub enabled: bool,
    pub visible_in_tray: bool,
    /// Sent when the action is chosen.
    pub message: M,
}

impl<M> DeviceAction<M> {
    pub fn map<N>(self, f: impl FnOnce(M) -> N) -> DeviceAction<N> {
        DeviceAction {
            id: self.id,
            label: self.label,
            icon: self.icon,
            enabled: self.enabled,
            visible_in_tray: self.visible_in_tray,
            message: f(self.message),
        }
    }
}

/// A function the shell calls later with what it got (picked files, a
/// typed name), to make a plugin's message.
pub type Callback<A, M> = Arc<dyn Fn(A) -> M + Send + Sync>;

/// Checks a typed value: the error to show under the field, if any.
pub type Validator = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Files dropped where a plugin takes them.
#[derive(Clone)]
pub struct DropTarget<M> {
    /// What a drop does, for the drop hint ("Drop to send").
    pub label: String,
    pub on_drop: Callback<Vec<PathBuf>, M>,
}

impl<M: 'static> DropTarget<M> {
    pub fn map<N>(self, f: impl Fn(M) -> N + Send + Sync + 'static) -> DropTarget<N> {
        let on_drop = self.on_drop;
        DropTarget {
            label: self.label,
            on_drop: Arc::new(move |paths| f(on_drop(paths))),
        }
    }
}

impl<M> fmt::Debug for DropTarget<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DropTarget")
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

/// What a plugin asks of the shell: the things plugins share, which the
/// shell owns.
#[derive(Clone)]
pub enum ShellRequest<M> {
    /// A short message at the bottom of the window, with an optional button
    /// that goes somewhere.
    Toast {
        text: String,
        action: Option<(String, Route)>,
    },
    /// A toast while the window is focused, a desktop notification
    /// otherwise.
    Notify {
        title: String,
        body: String,
    },
    Navigate(Route),
    ShowWindow,
    /// Pick files to open; `then` gets them, and isn't called on cancel.
    PickFiles {
        title: String,
        confirm_label: String,
        then: Callback<Vec<PathBuf>, M>,
    },
    /// Ask before doing something; `then` is sent on confirm.
    Confirm {
        title: String,
        body: String,
        confirm_label: String,
        then: M,
    },
    /// Ask for a line of text. `validate` gives the error to show under the
    /// field, if any; `then` gets the text once it is valid.
    Prompt {
        title: String,
        label: String,
        initial: String,
        confirm_label: String,
        validate: Validator,
        then: Callback<String, M>,
    },
}

impl<M: 'static> ShellRequest<M> {
    /// A toast with no button.
    pub fn toast(text: impl Into<String>) -> Self {
        Self::Toast {
            text: text.into(),
            action: None,
        }
    }

    pub fn map<N>(self, f: impl Fn(M) -> N + Clone + Send + Sync + 'static) -> ShellRequest<N> {
        match self {
            Self::Toast { text, action } => ShellRequest::Toast { text, action },
            Self::Notify { title, body } => ShellRequest::Notify { title, body },
            Self::Navigate(route) => ShellRequest::Navigate(route),
            Self::ShowWindow => ShellRequest::ShowWindow,
            Self::PickFiles {
                title,
                confirm_label,
                then,
            } => ShellRequest::PickFiles {
                title,
                confirm_label,
                then: Arc::new(move |paths| f(then(paths))),
            },
            Self::Confirm {
                title,
                body,
                confirm_label,
                then,
            } => ShellRequest::Confirm {
                title,
                body,
                confirm_label,
                then: f(then),
            },
            Self::Prompt {
                title,
                label,
                initial,
                confirm_label,
                validate,
                then,
            } => ShellRequest::Prompt {
                title,
                label,
                initial,
                confirm_label,
                validate,
                then: Arc::new(move |text| f(then(text))),
            },
        }
    }
}

impl<M: fmt::Debug> fmt::Debug for ShellRequest<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toast { text, action } => f
                .debug_struct("Toast")
                .field("text", text)
                .field("action", action)
                .finish(),
            Self::Notify { title, body } => f
                .debug_struct("Notify")
                .field("title", title)
                .field("body", body)
                .finish(),
            Self::Navigate(route) => f.debug_tuple("Navigate").field(route).finish(),
            Self::ShowWindow => f.write_str("ShowWindow"),
            Self::PickFiles { title, .. } => f
                .debug_struct("PickFiles")
                .field("title", title)
                .finish_non_exhaustive(),
            Self::Confirm { title, then, .. } => f
                .debug_struct("Confirm")
                .field("title", title)
                .field("then", then)
                .finish_non_exhaustive(),
            Self::Prompt { title, initial, .. } => f
                .debug_struct("Prompt")
                .field("title", title)
                .field("initial", initial)
                .finish_non_exhaustive(),
        }
    }
}

/// What a [`Command`] produces: a message for the plugin, or a request for
/// the shell.
#[derive(Debug, Clone)]
pub enum Outcome<M> {
    Plugin(M),
    Shell(ShellRequest<M>),
}

impl<M: 'static> Outcome<M> {
    pub fn map<N>(self, f: impl Fn(M) -> N + Clone + Send + Sync + 'static) -> Outcome<N> {
        match self {
            Self::Plugin(message) => Outcome::Plugin(f(message)),
            Self::Shell(request) => Outcome::Shell(request.map(f)),
        }
    }
}

/// Work a plugin asks for from `update` or `on_event`: an iced [`Task`]
/// whose results are messages back to the plugin or requests to the shell.
#[must_use]
pub struct Command<M>(Task<Outcome<M>>);

impl<M: Send + 'static> Command<M> {
    pub fn none() -> Self {
        Self(Task::none())
    }

    /// Send `message` back to the plugin.
    pub fn message(message: M) -> Self {
        Self(Task::done(Outcome::Plugin(message)))
    }

    pub fn shell(request: ShellRequest<M>) -> Self {
        Self(Task::done(Outcome::Shell(request)))
    }

    pub fn batch(commands: impl IntoIterator<Item = Self>) -> Self {
        Self(Task::batch(commands.into_iter().map(|command| command.0)))
    }

    pub fn map<N: Send + 'static>(
        self,
        f: impl Fn(M) -> N + Clone + Send + Sync + 'static,
    ) -> Command<N> {
        Command(self.0.map(move |outcome| outcome.map(f.clone())))
    }

    /// The task, for the shell to run.
    pub fn into_task(self) -> Task<Outcome<M>> {
        self.0
    }
}

/// What a plugin gets from the shell while it handles a message or event.
#[derive(Clone)]
pub struct UiContext {
    core: Core,
    runtime: tokio::runtime::Handle,
    window_focused: bool,
}

impl UiContext {
    pub(crate) fn new(core: Core, runtime: tokio::runtime::Handle) -> Self {
        Self {
            core,
            runtime,
            window_focused: true,
        }
    }

    pub fn core(&self) -> &Core {
        &self.core
    }

    /// The core as the plugin's own module sees it, to call its typed API.
    pub fn plugin_context(&self) -> PluginContext {
        self.core.plugin_context()
    }

    /// Run `future` on the daemon's runtime, where the core's sockets live
    /// (iced's executor has no tokio reactor), and send `then` of its
    /// result back to the plugin.
    ///
    /// Write `future` as an `async move` block: some tokio futures, such as
    /// `tokio::time::sleep`, need the runtime already when they are made,
    /// and `update` doesn't run on it.
    pub fn spawn<T, M>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
        then: impl FnOnce(T) -> M + Send + 'static,
    ) -> Command<M>
    where
        T: Send + 'static,
        M: Send + 'static,
    {
        // `map` takes an `FnMut`, but the future produces one output.
        let mut then = Some(then);
        Command(on_runtime(&self.runtime, future).map(move |output| {
            let then = then.take().expect("a future has one output");
            Outcome::Plugin(then(output))
        }))
    }

    /// The device as the core knows it now.
    pub fn device(&self, device_id: &str) -> Option<DeviceSnapshot> {
        self.core.device(device_id)
    }

    /// Every file transfer.
    pub fn transfers(&self) -> Vec<TransferSnapshot> {
        self.core.transfers().list()
    }

    pub fn window_focused(&self) -> bool {
        self.window_focused
    }

    pub(crate) fn set_window_focused(&mut self, focused: bool) {
        self.window_focused = focused;
    }
}

/// Run `future` on `runtime` as an iced task. Produces nothing if the task
/// panics or the runtime shuts down first.
pub(crate) fn on_runtime<T: Send + 'static>(
    runtime: &tokio::runtime::Handle,
    future: impl Future<Output = T> + Send + 'static,
) -> Task<T> {
    Task::future(runtime.spawn(future)).then(|result| match result {
        Ok(output) => Task::done(output),
        Err(error) => {
            tracing::warn!(%error, "UI task did not finish");
            Task::none()
        }
    })
}

#[cfg(test)]
mod tests {
    use iced_fonts::lucide;

    use super::*;
    use crate::{core::testing::handle, ui::testing};

    /// Counts, and asks the shell for a toast with the count.
    #[derive(Default)]
    struct Counter {
        count: u32,
    }

    #[derive(Debug, Clone, PartialEq)]
    enum CounterMessage {
        Add(u32),
        Added,
    }

    impl UiPlugin for Counter {
        type Message = CounterMessage;

        fn id(&self) -> &'static str {
            "counter"
        }

        fn device_actions(&self, device: &DeviceSnapshot) -> Vec<DeviceAction<CounterMessage>> {
            vec![DeviceAction {
                id: "add",
                label: format!("Add for {}", device.device_name),
                icon: lucide::plus,
                enabled: true,
                visible_in_tray: false,
                message: CounterMessage::Add(2),
            }]
        }

        fn update(&mut self, _ctx: &UiContext, message: CounterMessage) -> Command<CounterMessage> {
            match message {
                CounterMessage::Add(amount) => {
                    self.count += amount;
                    Command::batch([
                        Command::message(CounterMessage::Added),
                        Command::shell(ShellRequest::Confirm {
                            title: format!("{}", self.count),
                            body: String::new(),
                            confirm_label: "OK".into(),
                            then: CounterMessage::Add(1),
                        }),
                    ])
                }
                CounterMessage::Added => Command::none(),
            }
        }
    }

    #[tokio::test]
    async fn a_message_goes_round_the_erased_plugin_and_back() {
        let (core, _commands) = handle();
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        let mut plugin: Box<dyn ErasedUiPlugin> = Box::new(Counter::default());
        let device = testing::device("Phone");

        let [action] = plugin.device_actions(&device).try_into().unwrap();
        assert_eq!(action.label, "Add for Phone");
        assert_eq!(action.message.plugin(), "counter");

        let outcomes = testing::outputs(plugin.update(&ctx, action.message).into_task()).await;
        let [
            Outcome::Plugin(added),
            Outcome::Shell(ShellRequest::Confirm { title, then, .. }),
        ] = &outcomes[..]
        else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        assert_eq!(title, "2");
        assert_eq!(added.plugin(), "counter");
        assert_eq!(
            added.downcast::<CounterMessage>(),
            Some(CounterMessage::Added)
        );

        // The confirm's message comes back erased, and still works.
        let _ = plugin.update(&ctx, then.clone());
        let outcomes =
            testing::outputs(plugin.update(&ctx, action_message(&*plugin)).into_task()).await;
        let Outcome::Shell(ShellRequest::Confirm { title, .. }) = &outcomes[1] else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        assert_eq!(title, "5");
    }

    fn action_message(plugin: &dyn ErasedUiPlugin) -> PluginMessage {
        plugin
            .device_actions(&testing::device("Phone"))
            .remove(0)
            .message
    }

    #[tokio::test]
    #[should_panic(expected = "wrong plugin")]
    async fn a_message_for_another_plugin_is_a_bug() {
        let (core, _commands) = handle();
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        let mut plugin = Counter::default();
        let stray = PluginMessage::new("other", CounterMessage::Added);
        let _ = ErasedUiPlugin::update(&mut plugin, &ctx, stray);
    }

    #[tokio::test]
    async fn spawned_work_runs_on_the_daemon_runtime() {
        let (core, _commands) = handle();
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        let command = ctx.spawn(
            async {
                // Needs a tokio reactor: this would panic on iced's executor.
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                7
            },
            CounterMessage::Add,
        );
        let outcomes = testing::outputs(command.into_task()).await;
        assert!(matches!(
            outcomes[..],
            [Outcome::Plugin(CounterMessage::Add(7))]
        ));
    }
}
