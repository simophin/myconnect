//! Share's UI: the *Send files* action, and files dropped on a device that
//! takes them.

use std::{path::PathBuf, sync::Arc};

use iced::Task;
use iced_fonts::lucide;

use super::{DeviceAction, DropTarget, Feature};
use crate::{
    core::{DeviceReachability, DeviceSnapshot},
    plugins::share::{PACKET_TYPE, SendPathError, send_path},
    ui::{
        self, Origin,
        context::UiContext,
        error::{FileBatch, describe_code, describe_error, describe_file_failures},
        i18n::fl,
        shell,
    },
};

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

/// Listed for every device, enabled while it takes files.
pub fn device_actions(device: &DeviceSnapshot) -> Vec<DeviceAction> {
    vec![DeviceAction {
        id: "send-files",
        label: "Send files".into(),
        icon: lucide::file_up,
        enabled: accepts_files(device),
        visible_in_tray: true,
        message: Feature::Share(Message::Pick {
            device_id: device.device_id.clone(),
            name: device.device_name.clone(),
        }),
    }]
}

/// Files dropped on a device that takes them are sent to it.
pub fn drop_target(device: &DeviceSnapshot) -> Option<DropTarget> {
    if !accepts_files(device) {
        return None;
    }
    let device_id = device.device_id.clone();
    let name = device.device_name.clone();
    Some(DropTarget {
        label: format!("Drop to send to {name}"),
        on_drop: Arc::new(move |paths| {
            Feature::Share(Message::Send {
                device_id: device_id.clone(),
                name: name.clone(),
                paths,
            })
        }),
    })
}

pub(crate) fn update(ctx: &UiContext, message: Message, origin: Origin) -> Task<ui::Message> {
    match message {
        Message::Pick { device_id, name } => shell::pick_files(
            origin,
            format!("Send files to {name}"),
            Arc::new(move |paths| {
                Feature::Share(Message::Send {
                    device_id: device_id.clone(),
                    name: name.clone(),
                    paths,
                })
            }),
        ),
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
                return shell::failed(
                    origin,
                    format!("Couldn’t send to {name}"),
                    describe_code("device_not_connected"),
                );
            }
            ctx.spawn(
                async move {
                    let mut failures = Vec::new();
                    for path in paths {
                        if let Err(error) = send_path(&plugin_ctx, &device_id, &path).await {
                            failures.push((path, describe(&error)));
                        }
                    }
                    describe_file_failures(FileBatch::Send, &failures)
                },
                move |failures| {
                    ui::Message::Feature(Feature::Share(Message::Sent { name, failures }), origin)
                },
            )
        }
        Message::Sent {
            name,
            failures: Some(text),
        } => shell::failed(origin, format!("Couldn’t send to {name}"), text),
        Message::Sent { failures: None, .. } => Task::none(),
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
        SendPathError::File(_) => fl!("share-error-file-unreadable"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::testing::handle,
        ui::{shell::PickFiles, testing},
    };

    fn send_action(device: &DeviceSnapshot) -> DeviceAction {
        let [action] = device_actions(device).try_into().unwrap();
        action
    }

    /// The share message `feature` holds.
    fn share(feature: Feature) -> Message {
        let Feature::Share(message) = feature else {
            panic!("not a share message: {feature:?}");
        };
        message
    }

    #[test]
    fn sending_is_enabled_and_drops_taken_only_while_the_device_takes_files() {
        let mut device = testing::device("Pixel");
        assert!(!send_action(&device).enabled, "listed, but disabled");
        assert!(drop_target(&device).is_none());

        device.incoming_capabilities = vec![PACKET_TYPE.into()];
        assert!(send_action(&device).enabled);
        let target = drop_target(&device).unwrap();
        assert_eq!(target.label, "Drop to send to Pixel");
        let Message::Send {
            device_id,
            name,
            paths,
        } = share((target.on_drop)(vec!["/tmp/a.txt".into()]))
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
        assert!(drop_target(&device).is_none());
    }

    #[tokio::test]
    async fn the_action_asks_for_files_then_sends_them() {
        let (core, _commands) = handle();
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        let mut device = testing::device("Pixel");
        device.incoming_capabilities = vec![PACKET_TYPE.into()];

        let outcomes = testing::outputs(update(
            &ctx,
            share(send_action(&device).message),
            Origin::Tray,
        ))
        .await;
        let [
            ui::Message::PickFiles(PickFiles {
                title,
                then,
                origin: Origin::Tray,
            }),
        ] = &outcomes[..]
        else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        assert_eq!(title, "Send files to Pixel");
        let Message::Send {
            device_id,
            name,
            paths,
        } = share(then(vec!["/tmp/a.txt".into()]))
        else {
            panic!("the picked files are sent");
        };
        assert_eq!(
            (device_id, name),
            (device.device_id.clone(), "Pixel".into())
        );
        assert_eq!(paths, [PathBuf::from("/tmp/a.txt")]);
    }

    /// What sending `paths` to `device` leads to: the failures' sentence.
    async fn failures(ctx: &UiContext, device: &DeviceSnapshot, paths: Vec<PathBuf>) -> String {
        let send = Message::Send {
            device_id: device.device_id.clone(),
            name: device.device_name.clone(),
            paths,
        };
        let sent = testing::outputs(update(ctx, send, Origin::Window)).await;
        let [
            ui::Message::Feature(
                Feature::Share(Message::Sent {
                    failures: Some(text),
                    ..
                }),
                Origin::Window,
            ),
        ] = &sent[..]
        else {
            panic!("unexpected outcomes: {sent:?}");
        };
        text.clone()
    }

    #[tokio::test]
    async fn failed_sends_are_reported_once() {
        let (core, _commands) = handle();
        let (device, _sent) = testing::connect_peer(&core, testing::PEER_ID, &[]);
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        let folder = tempfile::tempdir().unwrap();
        let photo = folder.path().join("photo.jpg");
        std::fs::write(&photo, "jpg").unwrap();

        // The peer doesn't take files.
        assert_eq!(
            failures(&ctx, &device, vec![photo.clone(), folder.path().into()]).await,
            "Couldn’t send 2 files: The device doesn’t support that."
        );
        let text = failures(&ctx, &device, vec![folder.path().join("gone.txt")]).await;
        assert_eq!(text, "Couldn’t send gone.txt: The file couldn’t be read.");

        let report = testing::outputs(update(
            &ctx,
            Message::Sent {
                name: "Peer".into(),
                failures: Some(text.clone()),
            },
            Origin::Window,
        ))
        .await;
        assert!(matches!(
            &report[..],
            [ui::Message::Report { text: said, failure: Some(title), .. }]
                if *said == text && title == "Couldn’t send to Peer"
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
        let outcomes = testing::outputs(update(&ctx, send, Origin::Window)).await;
        let [
            ui::Message::Report {
                text,
                failure: Some(title),
                ..
            },
        ] = &outcomes[..]
        else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        assert_eq!(title, "Couldn’t send to Pixel");
        assert_eq!(text, "The device is not connected right now.");
    }
}
