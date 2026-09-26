//! The Settings page: the daemon's settings, which are where every user
//! preference lives, each saved as soon as it changes, and starting on
//! login, which the system keeps. Features add their own sections (clipboard: "Sync clipboard") through
//! [`settings_sections`](crate::ui::features::Features::settings_sections).

use iced::{
    Element, Length,
    widget::{column, scrollable, text},
};
use iced_fonts::lucide;

use crate::{
    core::SettingsSnapshot,
    ui::{
        i18n::fl,
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
    pub set_start_on_login: fn(bool) -> M,
    /// Open the About page.
    pub about: M,
}

/// The settings `store` holds, with the features' `sections` of them
/// after the download folder, and a link to About. `version` is the app's, and
/// `start_on_login` whether the system starts it at login.
pub fn view<'a, M: Clone + 'a>(
    store: &'a Store,
    sections: impl FnOnce(&'a SettingsSnapshot) -> Vec<Element<'a, M>>,
    version: &'a str,
    start_on_login: bool,
    actions: Actions<M>,
) -> Element<'a, M> {
    let header = widgets::page_header(fl!("settings-title"), Some(actions.back.clone()), vec![]);
    let body = match store.settings() {
        Load::Loading => widgets::loading(fl!("settings-loading")),
        Load::Failed(error) => widgets::error_view(error.as_str(), Some(actions.retry)),
        Load::Loaded(settings) => list(
            settings,
            sections(settings),
            version,
            start_on_login,
            actions,
        ),
    };
    widgets::page(header, body)
}

fn list<'a, M: Clone + 'a>(
    settings: &'a SettingsSnapshot,
    sections: Vec<Element<'a, M>>,
    version: &'a str,
    start_on_login: bool,
    actions: Actions<M>,
) -> Element<'a, M> {
    let edit = || Some(lucide::pencil().size(16).style(text::secondary).into());
    let mut items = column![
        widgets::setting(
            lucide::monitor,
            fl!("settings-device-name"),
            settings.device_name.as_str(),
            edit(),
            Some(actions.rename),
        ),
        widgets::setting(
            lucide::folder,
            fl!("settings-download-dir"),
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
            fl!("settings-close-to-tray"),
            fl!("settings-close-to-tray-detail"),
            settings.close_to_tray,
            actions.set_close_to_tray,
        ))
        .push(widgets::switch_setting(
            lucide::power,
            fl!("settings-start-on-login"),
            fl!("settings-start-on-login-detail"),
            start_on_login,
            actions.set_start_on_login,
        ))
        .push(widgets::setting(
            lucide::info,
            fl!("settings-about"),
            fl!("settings-version", version = version),
            Some(
                lucide::chevron_right()
                    .size(16)
                    .style(text::secondary)
                    .into(),
            ),
            Some(actions.about),
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
        StartOnLogin(bool),
        Section(bool),
        About,
    }

    fn actions() -> Actions<Message> {
        Actions {
            back: Message::Back,
            retry: Message::Retry,
            rename: Message::Rename,
            choose_download_dir: Message::Choose,
            set_close_to_tray: Message::CloseToTray,
            set_start_on_login: Message::StartOnLogin,
            about: Message::About,
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
        let mut ui = Simulator::new(view(store, sections, "1.2.3 (dev)", false, actions()));
        ui.click(target).unwrap();
        ui.into_messages().collect()
    }

    #[test]
    fn every_setting_is_shown_with_its_value() {
        let store = store();
        let mut ui = Simulator::new(view(&store, sections, "1.2.3 (dev)", false, actions()));
        for shown in [
            "Device name",
            "Desktop",
            "Save received files in",
            "/home/me/Downloads",
            "Sync clipboard",
            "Keep running when the window is closed",
            "Start when you log in",
            "About Ferry",
            "Version 1.2.3 (dev)",
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
        assert_eq!(
            clicked(&store, "Start when you log in"),
            [Message::StartOnLogin(true)]
        );
        assert_eq!(clicked(&store, "Sync clipboard"), [Message::Section(false)]);
        assert_eq!(clicked(&store, "About Ferry"), [Message::About]);
        assert_eq!(
            clicked(&store, iced::widget::Id::from("Back")),
            [Message::Back]
        );
    }

    #[test]
    fn loading_and_failure_have_their_own_views() {
        let loading = Store::default();
        let mut ui = Simulator::new(view(&loading, sections, "", false, actions()));
        assert!(ui.find("Loading settings…").is_ok());

        let mut failed = Store::default();
        failed.apply_snapshot(Snapshot {
            devices: Ok(Vec::new()),
            pairings: Ok(Vec::new()),
            transfers: Vec::new(),
            settings: Err("Ferry isn’t running.".into()),
        });
        assert_eq!(clicked(&failed, "Retry"), [Message::Retry]);
    }

    #[test]
    fn snapshot_settings() {
        let store = store();
        testing::snapshot("settings", (440.0, 620.0), || {
            view(&store, sections, "v1.2.0", false, actions())
        });
    }
}
