//! Ping's UI half: the *Ping* action, and a notification when a device
//! pings this computer.

use iced_fonts::lucide;

use crate::plugins::ping::{PACKET_TYPE, ReceivedPing, send_ping};
use crate::{
    core::{CoreEvent, DeviceReachability, DeviceSnapshot, EventData},
    ui::{
        error::describe_error,
        plugin::{Command, DeviceAction, ShellRequest, UiContext, UiPlugin},
    },
};

pub struct PingUi;

#[derive(Debug, Clone)]
pub enum Message {
    /// Ping the device. Carries its name for the toast, since the device
    /// may be gone by the time the answer arrives.
    Ping { device_id: String, name: String },
}

impl UiPlugin for PingUi {
    type Message = Message;

    fn id(&self) -> &'static str {
        crate::plugins::ping::ID
    }

    /// Listed for every device, enabled while it is connected and takes
    /// pings.
    fn device_actions(&self, device: &DeviceSnapshot) -> Vec<DeviceAction<Message>> {
        vec![DeviceAction {
            id: "ping",
            label: "Ping".into(),
            icon: lucide::bell_ring,
            enabled: accepts_pings(device),
            visible_in_tray: true,
            message: Message::Ping {
                device_id: device.device_id.clone(),
                name: device.device_name.clone(),
            },
        }]
    }

    fn on_event(&mut self, _ctx: &UiContext, event: &CoreEvent) -> Command<Message> {
        let EventData::Plugin(event) = &event.event else {
            return Command::none();
        };
        match event.decode::<ReceivedPing>() {
            Some(ping) => Command::shell(ShellRequest::Notify {
                title: ping.device_name,
                body: ping.message.unwrap_or_else(|| "Ping!".into()),
            }),
            None => Command::none(),
        }
    }

    fn update(&mut self, ctx: &UiContext, message: Message) -> Command<Message> {
        match message {
            Message::Ping { device_id, name } => {
                // Queues the packet; nothing here waits on the network.
                Command::shell(match send_ping(&ctx.plugin_context(), &device_id, None) {
                    Ok(()) => ShellRequest::done(format!("Pinged {name}.")),
                    Err(error) => ShellRequest::failed(
                        format!("Couldn’t ping {name}"),
                        describe_error(&error),
                    ),
                })
            }
        }
    }
}

/// Whether a ping sent now would be accepted.
fn accepts_pings(device: &DeviceSnapshot) -> bool {
    device.reachability == DeviceReachability::Connected
        && device
            .incoming_capabilities
            .iter()
            .any(|capability| capability == PACKET_TYPE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::{PluginEvent, testing::handle},
        ui::{plugin::Outcome, testing},
    };

    fn ping_action(device: &DeviceSnapshot) -> DeviceAction<Message> {
        let [action] = PingUi.device_actions(device).try_into().unwrap();
        action
    }

    #[test]
    fn ping_is_enabled_only_for_a_connected_device_that_takes_pings() {
        let mut device = testing::device("Pixel");
        assert!(!ping_action(&device).enabled, "listed, but disabled");
        device.incoming_capabilities = vec![PACKET_TYPE.into()];
        assert!(ping_action(&device).enabled);
        device.reachability = DeviceReachability::Discovered;
        assert!(!ping_action(&device).enabled);
    }

    #[tokio::test]
    async fn pinging_sends_a_ping_and_says_so() {
        let (core, _commands) = handle();
        let (device, mut sent) = testing::connect_peer(&core, testing::PEER_ID, &[PACKET_TYPE]);
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        let message = ping_action(&device).message;
        let outcomes = testing::outputs(PingUi.update(&ctx, message).into_task()).await;
        let [
            Outcome::Shell(ShellRequest::Report {
                text,
                failure: None,
            }),
        ] = &outcomes[..]
        else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        assert_eq!(text, "Pinged Peer.");
        assert_eq!(sent.try_recv().unwrap().packet_type, PACKET_TYPE);
    }

    #[tokio::test]
    async fn a_ping_to_a_device_that_is_gone_says_why() {
        let (core, _commands) = handle();
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        let message = ping_action(&testing::device("Pixel")).message;
        let outcomes = testing::outputs(PingUi.update(&ctx, message).into_task()).await;
        let [
            Outcome::Shell(ShellRequest::Report {
                text,
                failure: Some(title),
            }),
        ] = &outcomes[..]
        else {
            panic!("unexpected outcomes: {outcomes:?}");
        };
        assert_eq!(title, "Couldn’t ping Pixel");
        assert_eq!(text, "That device is no longer known.");
    }

    #[tokio::test]
    async fn a_received_ping_notifies_with_its_message() {
        let (core, _commands) = handle();
        let ctx = UiContext::new(core, tokio::runtime::Handle::current());
        let received = |message: Option<&str>| CoreEvent {
            sequence: 1,
            timestamp: 0,
            event: EventData::Plugin(
                PluginEvent::new(&ReceivedPing {
                    device_id: "pixel".into(),
                    device_name: "Pixel".into(),
                    message: message.map(Into::into),
                })
                .unwrap(),
            ),
        };
        for (message, body) in [(Some("Dinner!"), "Dinner!"), (None, "Ping!")] {
            let outcomes =
                testing::outputs(PingUi.on_event(&ctx, &received(message)).into_task()).await;
            let [Outcome::Shell(ShellRequest::Notify { title, body: said })] = &outcomes[..] else {
                panic!("unexpected outcomes: {outcomes:?}");
            };
            assert_eq!((title.as_str(), said.as_str()), ("Pixel", body));
        }
    }
}
