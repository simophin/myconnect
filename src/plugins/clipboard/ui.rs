//! The clipboard feature's UI half: the *Send clipboard* action, and the
//! "Sync clipboard" setting.

use std::sync::Arc;

use iced_fonts::lucide;

use super::{ClipboardPlugin, ClipboardSettings, ClipboardSyncError, PACKET_TYPE};
use crate::{
    core::{DeviceReachability, DeviceSnapshot, SettingsSnapshot},
    ui::{
        error::{describe_code, describe_error as describe_core_error},
        plugin::{Command, DeviceAction, ShellRequest, UiContext, UiPlugin},
        widgets,
    },
};

/// The UI half, over the same plugin instance the core runs.
pub struct ClipboardUi {
    plugin: Arc<ClipboardPlugin>,
}

impl ClipboardUi {
    pub fn new(plugin: Arc<ClipboardPlugin>) -> Self {
        Self { plugin }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Send this computer's clipboard to the device. Carries its name for
    /// the toast, since the device may be gone by the time it is sent.
    Send { device_id: String, name: String },
    /// How sending went: what to say, or why it failed, and to which
    /// device.
    Sent {
        name: String,
        result: Result<String, String>,
    },
    /// Turn syncing on or off.
    SetSync(bool),
    /// Why changing the setting failed, if it did. Its event updates the
    /// switch.
    SyncSaved(Option<String>),
}

impl UiPlugin for ClipboardUi {
    type Message = Message;

    fn id(&self) -> &'static str {
        super::ID
    }

    /// Listed for a device that takes clipboard text at all, enabled while
    /// it is paired and connected.
    fn device_actions(&self, device: &DeviceSnapshot) -> Vec<DeviceAction<Message>> {
        let supported = device
            .incoming_capabilities
            .iter()
            .any(|capability| capability == PACKET_TYPE);
        if !supported {
            return Vec::new();
        }
        vec![DeviceAction {
            id: "send-clipboard",
            label: "Send clipboard".into(),
            icon: lucide::clipboard_paste,
            enabled: device.paired && device.reachability == DeviceReachability::Connected,
            visible_in_tray: true,
            message: Message::Send {
                device_id: device.device_id.clone(),
                name: device.device_name.clone(),
            },
        }]
    }

    fn update(&mut self, ctx: &UiContext, message: Message) -> Command<Message> {
        match message {
            Message::Send { device_id, name } => {
                let plugin = self.plugin.clone();
                let plugin_ctx = ctx.plugin_context();
                // Reading the system clipboard can block for a while.
                ctx.spawn(
                    async move {
                        tokio::task::spawn_blocking(move || plugin.send_to(&plugin_ctx, &device_id))
                            .await
                    },
                    move |result| {
                        let result = match result {
                            Ok(Ok(())) => Ok(format!("Sent the clipboard to {name}.")),
                            Ok(Err(error)) => Err(describe_error(&error)),
                            Err(_) => Err(describe_code("internal")),
                        };
                        Message::Sent { name, result }
                    },
                )
            }
            Message::Sent { name, result } => Command::shell(match result {
                Ok(text) => ShellRequest::done(text),
                Err(error) => {
                    ShellRequest::failed(format!("Couldn’t send the clipboard to {name}"), error)
                }
            }),
            Message::SetSync(enabled) => {
                let core = ctx.core().clone();
                // Writes the settings file.
                ctx.spawn(
                    async move { core.update_settings(ClipboardSettings::sync_enabled_patch(enabled)) },
                    |result| Message::SyncSaved(result.err().map(|error| describe_core_error(&error))),
                )
            }
            Message::SyncSaved(None) => Command::none(),
            Message::SyncSaved(Some(error)) => Command::shell(ShellRequest::toast(error)),
        }
    }

    fn view_settings<'a>(
        &'a self,
        settings: &'a SettingsSnapshot,
    ) -> Option<iced::Element<'a, Message>> {
        Some(widgets::switch_setting(
            lucide::clipboard_copy,
            "Sync clipboard",
            "Share copied text with paired devices",
            ClipboardSettings::of(settings).sync_enabled,
            Message::SetSync,
        ))
    }
}

/// A sentence for the user about `error`.
pub fn describe_error(error: &ClipboardSyncError) -> String {
    describe(error.code())
}

/// Words this feature's codes and leaves the rest to the UI core.
fn describe(code: &str) -> String {
    match code {
        "clipboard_empty" => "There is no text on the clipboard to send.".into(),
        "clipboard_text_too_large" => "The clipboard text is too long to send.".into(),
        code => describe_code(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_test::simulator::Simulator;

    use crate::{
        core::{
            CoreError,
            testing::{handle, handle_with_plugin},
        },
        plugins::clipboard::{ClipboardService, InMemoryClipboard},
        ui::{plugin::Outcome, testing},
    };

    fn ui(text: &str) -> ClipboardUi {
        let clipboard = InMemoryClipboard::shared();
        clipboard.set(text).unwrap();
        ClipboardUi::new(Arc::new(ClipboardPlugin::new(clipboard)))
    }

    fn actions(device: &DeviceSnapshot) -> Vec<DeviceAction<Message>> {
        ui("").device_actions(device)
    }

    #[test]
    fn send_is_listed_if_supported_and_enabled_if_paired_and_connected() {
        let mut device = testing::device("Pixel");
        assert!(actions(&device).is_empty(), "not listed without support");
        device.incoming_capabilities = vec![PACKET_TYPE.into()];
        device.reachability = DeviceReachability::Unavailable;
        let [action] = actions(&device).try_into().unwrap();
        assert_eq!(action.label, "Send clipboard");
        assert!(!action.enabled, "listed, but disabled while unreachable");
        device.reachability = DeviceReachability::Connected;
        assert!(actions(&device)[0].enabled);
        device.paired = false;
        assert!(!actions(&device)[0].enabled);
    }

    /// Choose the action on `device` and return what it reports: its text,
    /// and the failure's title.
    async fn send(
        ui: &mut ClipboardUi,
        ctx: &UiContext,
        device: &DeviceSnapshot,
    ) -> (String, Option<String>) {
        let message = ui.device_actions(device).remove(0).message;
        let mut outcomes = testing::outputs(ui.update(ctx, message).into_task()).await;
        let Some(Outcome::Plugin(sent)) = outcomes.pop() else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        let outcomes = testing::outputs(ui.update(ctx, sent).into_task()).await;
        let [Outcome::Shell(ShellRequest::Report { text, failure })] = &outcomes[..] else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        (text.clone(), failure.clone())
    }

    #[tokio::test]
    async fn sending_the_clipboard_says_so_or_why_not() {
        let (core, _commands) = handle();
        let (device, mut sent) = testing::connect_peer(&core, testing::PEER_ID, &[PACKET_TYPE]);
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());

        assert_eq!(
            send(&mut ui("hello"), &ctx, &device).await,
            ("Sent the clipboard to Peer.".into(), None)
        );
        assert_eq!(sent.try_recv().unwrap().packet_type, PACKET_TYPE);

        assert_eq!(
            send(&mut ui(""), &ctx, &device).await,
            (
                "There is no text on the clipboard to send.".into(),
                Some("Couldn’t send the clipboard to Peer".into())
            )
        );
    }

    #[tokio::test]
    async fn the_sync_switch_shows_and_saves_the_setting() {
        let (core, plugin, _commands) =
            handle_with_plugin(ClipboardPlugin::new(InMemoryClipboard::shared()));
        let ctx = UiContext::new(core.clone(), tokio::runtime::Handle::current());
        let mut ui = ClipboardUi::new(plugin);
        let settings = core.settings().unwrap();
        assert!(
            ClipboardSettings::of(&settings).sync_enabled,
            "on by default"
        );

        let mut section = Simulator::new(ui.view_settings(&settings).unwrap());
        section.click("Sync clipboard").unwrap();
        let clicked: Vec<_> = section.into_messages().collect();
        let [Message::SetSync(false)] = &clicked[..] else {
            panic!("unexpected messages: {clicked:?}");
        };

        let outcomes = testing::outputs(ui.update(&ctx, clicked[0].clone()).into_task()).await;
        assert!(matches!(
            outcomes[..],
            [Outcome::Plugin(Message::SyncSaved(None))]
        ));
        assert!(!ClipboardSettings::of(&core.settings().unwrap()).sync_enabled);
    }

    #[tokio::test]
    async fn a_setting_that_cant_be_saved_says_why() {
        // This core doesn't run the clipboard plugin, so it has no section.
        let (core, _commands) = handle();
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        let mut ui = ui("");
        let mut outcomes =
            testing::outputs(ui.update(&ctx, Message::SetSync(false)).into_task()).await;
        let Some(Outcome::Plugin(saved)) = outcomes.pop() else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        let outcomes = testing::outputs(ui.update(&ctx, saved).into_task()).await;
        let [Outcome::Shell(ShellRequest::Toast { text, .. })] = &outcomes[..] else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        assert_eq!(text, &describe_code("invalid_settings"));
    }

    /// The clipboard codes `ui/lib/src/core/api/api_exception.dart` words.
    #[test]
    fn codes_read_like_the_flutter_app() {
        assert_eq!(
            describe_error(&ClipboardSyncError::Empty),
            "There is no text on the clipboard to send."
        );
        assert_eq!(
            describe_error(&ClipboardSyncError::TextTooLarge { limit: 1 }),
            "The clipboard text is too long to send."
        );
        assert_eq!(
            describe_error(&ClipboardSyncError::Core(CoreError::UnsupportedByPeer)),
            "The device doesn’t support that."
        );
    }
}
