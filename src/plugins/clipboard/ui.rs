//! The clipboard feature's UI half: the *Send clipboard* action.

use std::sync::Arc;

use iced_fonts::lucide;

use super::{ClipboardPlugin, ClipboardSyncError, PACKET_TYPE};
use crate::{
    core::{DeviceReachability, DeviceSnapshot},
    ui::{
        error::describe_code,
        plugin::{Command, DeviceAction, ShellRequest, UiContext, UiPlugin},
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
    /// How sending went: the toast to show.
    Sent(String),
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
                        Message::Sent(match result {
                            Ok(Ok(())) => format!("Sent the clipboard to {name}."),
                            Ok(Err(error)) => describe_error(&error),
                            Err(_) => describe_code("internal"),
                        })
                    },
                )
            }
            Message::Sent(text) => Command::shell(ShellRequest::toast(text)),
        }
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
    use crate::{
        core::{CoreError, testing::handle},
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

    /// Choose the action on `device` and return the toast it ends with.
    async fn send(ui: &mut ClipboardUi, ctx: &UiContext, device: &DeviceSnapshot) -> String {
        let message = ui.device_actions(device).remove(0).message;
        let mut outcomes = testing::outputs(ui.update(ctx, message).into_task()).await;
        let Some(Outcome::Plugin(sent)) = outcomes.pop() else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        let outcomes = testing::outputs(ui.update(ctx, sent).into_task()).await;
        let [Outcome::Shell(ShellRequest::Toast { text, .. })] = &outcomes[..] else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        text.clone()
    }

    #[tokio::test]
    async fn sending_the_clipboard_says_so_or_why_not() {
        let (core, _commands) = handle();
        let (device, mut sent) = testing::connect_peer(&core, testing::PEER_ID, &[PACKET_TYPE]);
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());

        assert_eq!(
            send(&mut ui("hello"), &ctx, &device).await,
            "Sent the clipboard to Peer."
        );
        assert_eq!(sent.try_recv().unwrap().packet_type, PACKET_TYPE);

        assert_eq!(
            send(&mut ui(""), &ctx, &device).await,
            "There is no text on the clipboard to send."
        );
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
