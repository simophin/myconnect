//! Desktop notifications, for what the user should see while the window is
//! closed or unfocused. A click on one shows the window
//! ([`DesktopEvent::NotificationClicked`]).
//!
//! On Linux the shell talks to `org.freedesktop.Notifications` itself, so it
//! can withdraw a notification and hear its click without a thread per
//! notification (ADR 0001). macOS and Windows have none yet (plan step
//! 13b).

use std::sync::Arc;

use super::Events;

/// Shows and withdraws notifications. `id` is the shell's own number for a
/// notification, to withdraw it later.
pub trait Notifier: Send + Sync + 'static {
    fn show(&self, id: u32, title: &str, body: &str);
    fn withdraw(&self, id: u32);
}

/// No notifications: they are logged.
pub struct NoNotifier;

impl Notifier for NoNotifier {
    fn show(&self, _id: u32, title: &str, body: &str) {
        tracing::info!(title, body, "no desktop notifications here");
    }

    fn withdraw(&self, _id: u32) {}
}

/// The platform's notifications, reporting clicks to `events`.
pub fn start(runtime: &tokio::runtime::Handle, events: Events) -> Arc<dyn Notifier> {
    #[cfg(target_os = "linux")]
    {
        Arc::new(dbus::DbusNotifier::start(runtime, events))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (runtime, events);
        Arc::new(NoNotifier)
    }
}

#[cfg(target_os = "linux")]
mod dbus {
    use std::collections::HashMap;

    use futures_util::StreamExt;
    use tokio::sync::mpsc;
    use zbus::{Connection, Proxy, zvariant::Value};

    use super::{
        super::{APP_ID, DesktopEvent},
        Events, Notifier,
    };

    const DESTINATION: &str = "org.freedesktop.Notifications";
    const PATH: &str = "/org/freedesktop/Notifications";

    enum Request {
        Show {
            id: u32,
            title: String,
            body: String,
        },
        Withdraw(u32),
    }

    /// Talks to the notification server on the daemon's runtime, one
    /// request at a time, so a withdrawal never overtakes its show.
    pub struct DbusNotifier {
        requests: mpsc::UnboundedSender<Request>,
    }

    impl DbusNotifier {
        pub fn start(runtime: &tokio::runtime::Handle, events: Events) -> Self {
            let (requests, received) = mpsc::unbounded_channel();
            runtime.spawn(async move {
                if let Err(error) = serve(received, events).await {
                    tracing::warn!(%error, "desktop notifications unavailable");
                }
            });
            Self { requests }
        }
    }

    impl Notifier for DbusNotifier {
        fn show(&self, id: u32, title: &str, body: &str) {
            let _ = self.requests.send(Request::Show {
                id,
                title: title.into(),
                body: body.into(),
            });
        }

        fn withdraw(&self, id: u32) {
            let _ = self.requests.send(Request::Withdraw(id));
        }
    }

    async fn serve(
        mut requests: mpsc::UnboundedReceiver<Request>,
        events: Events,
    ) -> zbus::Result<()> {
        let connection = Connection::session().await?;
        let server = Proxy::new(&connection, DESTINATION, PATH, DESTINATION).await?;
        let mut invoked = server.receive_signal("ActionInvoked").await?;
        let mut closed = server.receive_signal("NotificationClosed").await?;
        // The shell's id for each notification showing, by the server's.
        let mut showing: HashMap<u32, u32> = HashMap::new();
        loop {
            tokio::select! {
                request = requests.recv() => match request {
                    None => return Ok(()),
                    Some(Request::Show { id, title, body }) => {
                        // Lets the server show the app's name and icon
                        // from its `.desktop` file.
                        let hints: HashMap<&str, Value<'_>> =
                            HashMap::from([("desktop-entry", Value::from(APP_ID))]);
                        let shown = server
                            .call::<_, _, u32>(
                                "Notify",
                                &(
                                    "MyConnect",
                                    0_u32,
                                    "",
                                    title.as_str(),
                                    body.as_str(),
                                    // A click on the body is `default`.
                                    vec!["default", "Open"],
                                    hints,
                                    -1_i32,
                                ),
                            )
                            .await;
                        match shown {
                            Ok(server_id) => {
                                showing.insert(server_id, id);
                            }
                            Err(error) => tracing::warn!(%error, "notification not shown"),
                        }
                    }
                    Some(Request::Withdraw(id)) => {
                        let server_id = showing
                            .iter()
                            .find_map(|(server_id, shown)| (*shown == id).then_some(*server_id));
                        if let Some(server_id) = server_id {
                            showing.remove(&server_id);
                            if let Err(error) = server
                                .call::<_, _, ()>("CloseNotification", &(server_id,))
                                .await
                            {
                                tracing::debug!(%error, "notification not withdrawn");
                            }
                        }
                    }
                },
                Some(signal) = invoked.next() => {
                    let Ok((server_id, action)) = signal.body().deserialize::<(u32, String)>() else {
                        continue;
                    };
                    if showing.contains_key(&server_id) && action == "default" {
                        let _ = events.send(DesktopEvent::NotificationClicked);
                    }
                }
                Some(signal) = closed.next() => {
                    if let Ok((server_id, _reason)) = signal.body().deserialize::<(u32, u32)>() {
                        showing.remove(&server_id);
                    }
                }
            }
        }
    }
}
