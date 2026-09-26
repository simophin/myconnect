//! Files dropped on the window or the tray icon: where they go, and the
//! chooser that asks when the page doesn't say.

use std::path::PathBuf;

use iced::{Element, Task};

use super::{
    App, Message, Origin, Phase, error, features::DropTarget, overlay::drop as dropping,
    route::Route,
};

impl App {
    /// Files were dropped on the window: send them where the page says, if
    /// a feature takes them there, or ask where. Folders are refused.
    pub(super) fn dropped(&mut self, paths: Vec<PathBuf>) -> Task<Message> {
        let files = match self.sendable(paths) {
            Ok(files) => files,
            Err(refused) => return refused,
        };
        match self.drop_target_here() {
            Some(target) => Task::done(Message::Feature((target.on_drop)(files), Origin::Window)),
            None => {
                self.choosing = Some(files);
                Task::none()
            }
        }
    }

    /// Files were dropped on the tray icon: show the window, asking which
    /// device to send them to, whatever page it shows.
    pub(super) fn dropped_on_tray(&mut self, paths: Vec<PathBuf>) -> Task<Message> {
        let shown = self.show_window();
        // The pairing prompt is modal: the chooser would open under it.
        if self.incoming_prompt_shows() {
            return shown;
        }
        match self.sendable(paths) {
            Ok(files) => {
                self.choosing = Some(files);
                shown
            }
            Err(refused) => Task::batch([shown, refused]),
        }
    }

    /// The files among dropped `paths`, or what to do instead if there are
    /// none. Folders can't be sent, and some drops aren't local files.
    fn sendable(&mut self, paths: Vec<PathBuf>) -> Result<Vec<PathBuf>, Task<Message>> {
        if self.running().is_none() {
            return Err(Task::none());
        }
        let files: Vec<_> = paths
            .iter()
            .filter(|path| path.is_file())
            .cloned()
            .collect();
        if files.is_empty() {
            if paths.is_empty() {
                return Err(Task::none());
            }
            return Err(self.toast("Only files can be sent, not folders.".into(), None));
        }
        Ok(files)
    }

    /// The chooser's device was picked: open its page and hand it the
    /// files.
    pub(super) fn drop_on(&mut self, device_id: String) -> Task<Message> {
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
    pub(super) fn drop_target_here(&self) -> Option<DropTarget> {
        self.drop_target(self.route.device()?, &self.route)
    }

    /// What files dropped on `device_id` while the window shows `route`
    /// would do, if a feature takes them.
    pub(super) fn drop_target(&self, device_id: &str, route: &Route) -> Option<DropTarget> {
        let Phase::Running(running) = &self.phase else {
            return None;
        };
        let device = running.ctx.device(device_id)?;
        running.features.drop_target(device, route)
    }

    /// What the drop hint says while files hover over the window, if they
    /// do.
    pub(super) fn drop_hint(&self) -> Option<String> {
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
    pub(super) fn chooser(&self) -> Option<Element<'_, Message>> {
        let files = self.choosing.as_ref()?;
        Some(dropping::chooser(
            files,
            self.recipients(),
            Message::DropOn,
            Message::CancelDrop,
        ))
    }

    /// The paired devices that would take dropped files now, by name.
    pub(super) fn recipients(&self) -> Vec<&crate::core::DeviceSnapshot> {
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
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iced::window;

    use super::*;
    use crate::{
        core::Core,
        ui::{KeyCommand, desktop::DesktopEvent, features::browse, testing, tests::*},
    };

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
        use crate::ui::features::browse::files::tests::{Call, phone_files};

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
    async fn files_dropped_on_the_tray_open_the_window_and_ask_where_to_go() {
        let mut sharing = Sharing::new();
        let peer = sharing.peer(testing::PEER_ID, true).await;
        // Even from a device's page, which a drop on the window would use.
        sharing.app.route = Route::Device(peer.clone());
        sharing.window(window::Event::Closed).await;
        assert!(sharing.app.window.is_none());

        let (photo, album) = (sharing.file("photo.jpg"), sharing.file("album"));
        settle(
            &mut sharing.app,
            Message::Desktop(DesktopEvent::TrayDropped(vec![album, photo])),
        )
        .await;
        assert!(sharing.app.window.is_some());
        assert!(shows(&sharing.app, "Send photo.jpg"));
        assert!(sharing.sent().is_empty());

        assert_eq!(sharing.app.recipients()[0].device_id, peer);
        // The chooser's entry; the page's own "Peer" is under it.
        settle(&mut sharing.app, Message::DropOn(peer.clone())).await;
        assert!(sharing.app.choosing.is_none());
        assert_eq!(sharing.sent(), [(peer, "photo.jpg".into())]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_folder_dropped_on_the_tray_is_refused() {
        let mut sharing = Sharing::new();
        let _peer = sharing.peer(testing::PEER_ID, true).await;

        let album = sharing.file("album");
        settle(
            &mut sharing.app,
            Message::Desktop(DesktopEvent::TrayDropped(vec![album])),
        )
        .await;
        assert!(sharing.app.choosing.is_none());
        assert_eq!(
            sharing.app.toasts.items()[0].text,
            "Only files can be sent, not folders."
        );
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
}
