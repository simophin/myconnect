//! Share's UI half: the *Send files* action, and files dropped on a device
//! that takes them.

use std::{path::PathBuf, sync::Arc};

use iced_fonts::lucide;

use crate::plugins::share::{PACKET_TYPE, SendPathError, send_path};
use crate::{
    core::{DeviceReachability, DeviceSnapshot},
    ui::{
        error::{describe_code, describe_error, describe_file_failures},
        plugin::{Command, DeviceAction, DropTarget, ShellRequest, UiContext, UiPlugin},
        route::Route,
    },
};

pub struct ShareUi;

#[derive(Debug, Clone)]
pub enum Message {
    /// Pick files to send to the device. Carries its name for the picker's
    /// title.
    Pick { device_id: String, name: String },
    /// Send these files to the device, one at a time. Carries its name for
    /// the report.
    Send {
        device_id: String,
        name: String,
        paths: Vec<PathBuf>,
    },
    /// The files are sent, or the sentence about those that failed.
    Sent {
        name: String,
        failures: Option<String>,
    },
}

impl UiPlugin for ShareUi {
    type Message = Message;

    fn id(&self) -> &'static str {
        crate::plugins::share::ID
    }

    /// Listed for every device, enabled while it takes files.
    fn device_actions(&self, device: &DeviceSnapshot) -> Vec<DeviceAction<Message>> {
        vec![DeviceAction {
            id: "send-files",
            label: "Send files".into(),
            icon: lucide::file_up,
            enabled: accepts_files(device),
            visible_in_tray: true,
            message: Message::Pick {
                device_id: device.device_id.clone(),
                name: device.device_name.clone(),
            },
        }]
    }

    /// Files dropped on a device that takes them are sent to it.
    fn drop_target(&self, device: &DeviceSnapshot, _route: &Route) -> Option<DropTarget<Message>> {
        if !accepts_files(device) {
            return None;
        }
        let device_id = device.device_id.clone();
        let name = device.device_name.clone();
        Some(DropTarget {
            label: format!("Drop to send to {name}"),
            on_drop: Arc::new(move |paths| Message::Send {
                device_id: device_id.clone(),
                name: name.clone(),
                paths,
            }),
        })
    }

    fn update(&mut self, ctx: &UiContext, message: Message) -> Command<Message> {
        match message {
            Message::Pick { device_id, name } => Command::shell(ShellRequest::PickFiles {
                title: format!("Send files to {name}"),
                confirm_label: "Send".into(),
                then: Arc::new(move |paths| Message::Send {
                    device_id: device_id.clone(),
                    name: name.clone(),
                    paths,
                }),
            }),
            Message::Send {
                device_id,
                name,
                paths,
            } => {
                let plugin_ctx = ctx.plugin_context();
                // It may have gone while the user picked.
                let connected = plugin_ctx
                    .device(&device_id)
                    .is_some_and(|device| device.reachability == DeviceReachability::Connected);
                if !connected {
                    return Command::shell(ShellRequest::failed(
                        format!("Couldn’t send to {name}"),
                        describe_code("device_not_connected"),
                    ));
                }
                ctx.spawn(
                    async move {
                        let mut failures = Vec::new();
                        for path in paths {
                            if let Err(error) = send_path(&plugin_ctx, &device_id, &path).await {
                                failures.push((path, describe(&error)));
                            }
                        }
                        describe_file_failures("send", &failures)
                    },
                    move |failures| Message::Sent { name, failures },
                )
            }
            Message::Sent {
                name,
                failures: Some(text),
            } => Command::shell(ShellRequest::failed(
                format!("Couldn’t send to {name}"),
                text,
            )),
            Message::Sent { failures: None, .. } => Command::none(),
        }
    }
}

/// Whether a file sent now would be accepted.
fn accepts_files(device: &DeviceSnapshot) -> bool {
    device.reachability == DeviceReachability::Connected
        && device
            .incoming_capabilities
            .iter()
            .any(|capability| capability == PACKET_TYPE)
}

/// A sentence for the user about why a file wasn't sent.
fn describe(error: &SendPathError) -> String {
    match error {
        SendPathError::Core(error) => describe_error(error),
        SendPathError::File(_) => "The file couldn’t be read.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::testing::handle,
        ui::{plugin::Outcome, testing},
    };

    fn send_action(device: &DeviceSnapshot) -> DeviceAction<Message> {
        let [action] = ShareUi.device_actions(device).try_into().unwrap();
        action
    }

    #[test]
    fn sending_is_enabled_and_drops_taken_only_while_the_device_takes_files() {
        let mut device = testing::device("Pixel");
        assert!(!send_action(&device).enabled, "listed, but disabled");
        assert!(ShareUi.drop_target(&device, &Route::Devices).is_none());

        device.incoming_capabilities = vec![PACKET_TYPE.into()];
        assert!(send_action(&device).enabled);
        let target = ShareUi.drop_target(&device, &Route::Devices).unwrap();
        assert_eq!(target.label, "Drop to send to Pixel");
        let Message::Send {
            device_id,
            name,
            paths,
        } = (target.on_drop)(vec!["/tmp/a.txt".into()])
        else {
            panic!("a drop sends");
        };
        assert_eq!(
            (device_id, name),
            (device.device_id.clone(), "Pixel".into())
        );
        assert_eq!(paths, [PathBuf::from("/tmp/a.txt")]);

        device.reachability = DeviceReachability::Discovered;
        assert!(!send_action(&device).enabled);
        assert!(ShareUi.drop_target(&device, &Route::Devices).is_none());
    }

    #[tokio::test]
    async fn the_action_asks_for_files_then_sends_them() {
        let (core, _commands) = handle();
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        let mut device = testing::device("Pixel");
        device.incoming_capabilities = vec![PACKET_TYPE.into()];

        let outcomes = testing::outputs(
            ShareUi
                .update(&ctx, send_action(&device).message)
                .into_task(),
        )
        .await;
        let [
            Outcome::Shell(ShellRequest::PickFiles {
                title,
                confirm_label,
                then,
            }),
        ] = &outcomes[..]
        else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        assert_eq!(title, "Send files to Pixel");
        assert_eq!(confirm_label, "Send");
        let Message::Send {
            device_id,
            name,
            paths,
        } = then(vec!["/tmp/a.txt".into()])
        else {
            panic!("the picked files are sent");
        };
        assert_eq!(
            (device_id, name),
            (device.device_id.clone(), "Pixel".into())
        );
        assert_eq!(paths, [PathBuf::from("/tmp/a.txt")]);
    }

    #[tokio::test]
    async fn failed_sends_are_reported_once() {
        let (core, _commands) = handle();
        let (device, _sent) = testing::connect_peer(&core, testing::PEER_ID, &[]);
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        let folder = tempfile::tempdir().unwrap();
        let photo = folder.path().join("photo.jpg");
        std::fs::write(&photo, "jpg").unwrap();
        let send = |paths: Vec<PathBuf>| Message::Send {
            device_id: device.device_id.clone(),
            name: device.device_name.clone(),
            paths,
        };

        // The peer doesn't take files.
        let sent = testing::outputs(
            ShareUi
                .update(&ctx, send(vec![photo.clone(), folder.path().into()]))
                .into_task(),
        )
        .await;
        let [
            Outcome::Plugin(Message::Sent {
                failures: Some(text),
                ..
            }),
        ] = &sent[..]
        else {
            panic!("unexpected outcomes: {sent:?}");
        };
        assert_eq!(
            text,
            "Couldn’t send 2 files: The device doesn’t support that."
        );

        let sent = testing::outputs(
            ShareUi
                .update(&ctx, send(vec![folder.path().join("gone.txt")]))
                .into_task(),
        )
        .await;
        let [
            Outcome::Plugin(Message::Sent {
                failures: Some(text),
                ..
            }),
        ] = &sent[..]
        else {
            panic!("unexpected outcomes: {sent:?}");
        };
        assert_eq!(text, "Couldn’t send gone.txt: The file couldn’t be read.");

        let report = testing::outputs(
            ShareUi
                .update(
                    &ctx,
                    Message::Sent {
                        name: "Peer".into(),
                        failures: Some(text.clone()),
                    },
                )
                .into_task(),
        )
        .await;
        assert!(matches!(
            &report[..],
            [Outcome::Shell(ShellRequest::Report { text: said, failure: Some(title) })]
                if said == text && title == "Couldn’t send to Peer"
        ));
    }

    #[tokio::test]
    async fn a_device_gone_while_picking_says_so() {
        let (core, _commands) = handle();
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        let send = Message::Send {
            device_id: testing::PEER_ID.into(),
            name: "Pixel".into(),
            paths: vec!["/tmp/a.txt".into()],
        };
        let outcomes = testing::outputs(ShareUi.update(&ctx, send).into_task()).await;
        let [
            Outcome::Shell(ShellRequest::Report {
                text,
                failure: Some(title),
            }),
        ] = &outcomes[..]
        else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        assert_eq!(title, "Couldn’t send to Pixel");
        assert_eq!(text, "The device is not connected right now.");
    }
}
