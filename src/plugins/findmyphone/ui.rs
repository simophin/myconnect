//! Find my phone's UI half: the *Ring* action.

use iced_fonts::lucide;

use super::{REQUEST_PACKET_TYPE, ring_device};
use crate::{
    core::{DeviceReachability, DeviceSnapshot},
    ui::{
        error::describe_error,
        plugin::{Command, DeviceAction, ShellRequest, UiContext, UiPlugin},
    },
};

pub struct FindMyPhoneUi;

#[derive(Debug, Clone)]
pub enum Message {
    /// Ask the device to ring. Carries its name for the toast, since the
    /// device may be gone by the time the answer arrives.
    Ring { device_id: String, name: String },
}

impl UiPlugin for FindMyPhoneUi {
    type Message = Message;

    fn id(&self) -> &'static str {
        super::ID
    }

    /// Listed for every device, enabled while it is connected and can ring.
    /// The tray lists it only for a device that says it can ring.
    fn device_actions(&self, device: &DeviceSnapshot) -> Vec<DeviceAction<Message>> {
        vec![DeviceAction {
            id: "ring",
            label: "Ring".into(),
            icon: lucide::volume_two,
            enabled: can_ring(device),
            visible_in_tray: advertises_ring(device),
            message: Message::Ring {
                device_id: device.device_id.clone(),
                name: device.device_name.clone(),
            },
        }]
    }

    fn update(&mut self, ctx: &UiContext, message: Message) -> Command<Message> {
        match message {
            Message::Ring { device_id, name } => {
                // Queues the packet; nothing here waits on the network.
                Command::shell(match ring_device(&ctx.plugin_context(), &device_id) {
                    Ok(()) => ShellRequest::done(format!("Asked {name} to ring.")),
                    Err(error) => ShellRequest::failed(
                        format!("Couldn’t ring {name}"),
                        describe_error(&error),
                    ),
                })
            }
        }
    }
}

/// Whether a request to ring sent now would be accepted.
fn can_ring(device: &DeviceSnapshot) -> bool {
    device.reachability == DeviceReachability::Connected && advertises_ring(device)
}

fn advertises_ring(device: &DeviceSnapshot) -> bool {
    device
        .incoming_capabilities
        .iter()
        .any(|capability| capability == REQUEST_PACKET_TYPE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::testing::handle,
        plugins::ping,
        ui::{plugin::Outcome, testing},
    };

    fn ring_action(device: &DeviceSnapshot) -> DeviceAction<Message> {
        let [action] = FindMyPhoneUi.device_actions(device).try_into().unwrap();
        action
    }

    #[test]
    fn ring_is_enabled_only_for_a_connected_device_that_can_ring() {
        let mut device = testing::device("Pixel");
        device.incoming_capabilities = vec![ping::PACKET_TYPE.into()];
        assert!(!ring_action(&device).enabled, "listed, but disabled");
        assert!(!ring_action(&device).visible_in_tray);
        device.incoming_capabilities = vec![REQUEST_PACKET_TYPE.into()];
        assert!(ring_action(&device).enabled);
        assert!(ring_action(&device).visible_in_tray);
        device.reachability = DeviceReachability::Unavailable;
        assert!(!ring_action(&device).enabled);
    }

    /// What ringing reports: its text, and the failure's title.
    async fn report(ctx: &UiContext, message: Message) -> (String, Option<String>) {
        let outcomes = testing::outputs(FindMyPhoneUi.update(ctx, message).into_task()).await;
        let [Outcome::Shell(ShellRequest::Report { text, failure })] = &outcomes[..] else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        (text.clone(), failure.clone())
    }

    #[tokio::test]
    async fn ringing_asks_the_device_and_says_so() {
        let (core, _commands) = handle();
        let (device, mut sent) =
            testing::connect_peer(&core, testing::PEER_ID, &[REQUEST_PACKET_TYPE]);
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        assert_eq!(
            report(&ctx, ring_action(&device).message).await,
            ("Asked Peer to ring.".into(), None)
        );
        assert_eq!(sent.try_recv().unwrap().packet_type, REQUEST_PACKET_TYPE);
    }

    #[tokio::test]
    async fn a_device_that_cant_ring_says_so() {
        let (core, _commands) = handle();
        let (device, _sent) = testing::connect_peer(&core, testing::PEER_ID, &[ping::PACKET_TYPE]);
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        assert_eq!(
            report(&ctx, ring_action(&device).message).await,
            (
                "The device doesn’t support that.".into(),
                Some("Couldn’t ring Peer".into())
            )
        );
    }
}
