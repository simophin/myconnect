//! The Settings page: the daemon's settings, which are where every user
//! preference lives, each saved as soon as it changes. Features add their
//! own sections (clipboard: "Sync clipboard") through
//! [`settings_sections`](crate::ui::features::Features::settings_sections).

use iced::{
    Element, Length,
    widget::{column, scrollable, text},
};
use iced_fonts::lucide;

use crate::{
    core::SettingsSnapshot,
    ui::{
        store::{Load, Store},
        widgets,
    },
};

/// What the page's controls ask for.
pub struct Actions<M> {
    pub back: M,
    /// Read the core again after a failed snapshot.
    pub retry: M,
    /// Ask for a new device name.
    pub rename: M,
    /// Pick the folder received files are saved in.
    pub choose_download_dir: M,
    pub set_close_to_tray: fn(bool) -> M,
}

/// The settings `store` holds, with the features' `sections` of them
/// after the download folder. `version` is the app's.
pub fn view<'a, M: Clone + 'a>(
    store: &'a Store,
    sections: impl FnOnce(&'a SettingsSnapshot) -> Vec<Element<'a, M>>,
    version: &'a str,
    actions: Actions<M>,
) -> Element<'a, M> {
    let header = widgets::page_header("Settings", Some(actions.back.clone()), vec![]);
    let body = match store.settings() {
        Load::Loading => widgets::loading("Loading settings…"),
        Load::Failed(error) => widgets::error_view(error.as_str(), Some(actions.retry)),
        Load::Loaded(settings) => list(settings, sections(settings), version, actions),
    };
    widgets::page(header, body)
}

fn list<'a, M: Clone + 'a>(
    settings: &'a SettingsSnapshot,
    sections: Vec<Element<'a, M>>,
    version: &'a str,
    actions: Actions<M>,
) -> Element<'a, M> {
    let edit = || Some(lucide::pencil().size(16).style(text::secondary).into());
    let mut items = column![
        widgets::setting(
            lucide::monitor,
            "Device name",
            settings.device_name.as_str(),
            edit(),
            Some(actions.rename),
        ),
        widgets::setting(
            lucide::folder,
            "Save received files in",
            settings.download_dir.display().to_string(),
            edit(),
            Some(actions.choose_download_dir),
        ),
    ]
    .spacing(8);
    for section in sections {
        items = items.push(section);
    }
    items = items
        .push(widgets::switch_setting(
            lucide::minimize_two,
            "Keep running when the window is closed",
            "Stay in the tray so devices can still reach this computer",
            settings.close_to_tray,
            actions.set_close_to_tray,
        ))
        .push(widgets::setting(
            lucide::info,
            "Version",
            version,
            None,
            None,
        ));
    scrollable(items).spacing(6).height(Length::Fill).into()
}

#[cfg(test)]
mod tests {
    use iced_test::simulator::Simulator;

    use super::*;
    use crate::ui::{store::Snapshot, testing};

    #[derive(Debug, Clone, PartialEq)]
    enum Message {
        Back,
        Retry,
        Rename,
        Choose,
        CloseToTray(bool),
        Section(bool),
    }

    fn actions() -> Actions<Message> {
        Actions {
            back: Message::Back,
            retry: Message::Retry,
            rename: Message::Rename,
            choose_download_dir: Message::Choose,
            set_close_to_tray: Message::CloseToTray,
        }
    }

    /// A feature's section: a switch of its own.
    fn sections(_settings: &SettingsSnapshot) -> Vec<Element<'_, Message>> {
        vec![widgets::switch_setting(
            lucide::clipboard_copy,
            "Sync clipboard",
            "Share copied text with paired devices",
            true,
            Message::Section,
        )]
    }

    fn store() -> Store {
        testing::store("Desktop", Vec::new())
    }

    fn clicked<S>(store: &Store, target: S) -> Vec<Message>
    where
        S: iced_test::selector::Selector + Send,
        S::Output: iced_test::selector::Bounded + Clone + Send + Sync + 'static,
    {
        let mut ui = Simulator::new(view(store, sections, "1.2.3 (dev)", actions()));
        ui.click(target).unwrap();
        ui.into_messages().collect()
    }

    #[test]
    fn every_setting_is_shown_with_its_value() {
        let store = store();
        let mut ui = Simulator::new(view(&store, sections, "1.2.3 (dev)", actions()));
        for shown in [
            "Device name",
            "Desktop",
            "Save received files in",
            "/home/me/Downloads",
            "Sync clipboard",
            "Keep running when the window is closed",
            "Version",
            "1.2.3 (dev)",
        ] {
            assert!(ui.find(shown).is_ok(), "{shown}");
        }
    }

    #[test]
    fn each_setting_sends_its_message() {
        let store = store();
        assert_eq!(clicked(&store, "Device name"), [Message::Rename]);
        assert_eq!(clicked(&store, "Save received files in"), [Message::Choose]);
        // `testing::store` keeps running in the tray: a click turns it off.
        assert_eq!(
            clicked(&store, "Keep running when the window is closed"),
            [Message::CloseToTray(false)]
        );
        assert_eq!(clicked(&store, "Sync clipboard"), [Message::Section(false)]);
        assert_eq!(
            clicked(&store, iced::widget::Id::from("Back")),
            [Message::Back]
        );
    }

    #[test]
    fn loading_and_failure_have_their_own_views() {
        let loading = Store::default();
        let mut ui = Simulator::new(view(&loading, sections, "", actions()));
        assert!(ui.find("Loading settings…").is_ok());

        let mut failed = Store::default();
        failed.apply_snapshot(Snapshot {
            devices: Ok(Vec::new()),
            pairings: Ok(Vec::new()),
            transfers: Vec::new(),
            settings: Err("MyConnect isn’t running.".into()),
        });
        assert_eq!(clicked(&failed, "Retry"), [Message::Retry]);
    }

    #[test]
    fn snapshot_settings() {
        let store = store();
        testing::snapshot("settings", (440.0, 620.0), || {
            view(&store, sections, "0.1.0 (v1.1.0-19-geeba428)", actions())
        });
    }
}
