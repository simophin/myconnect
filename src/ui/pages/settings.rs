//! The Settings page: the daemon's settings, which are where every user
//! preference lives, each saved as soon as it changes, starting on
//! login, which the system keeps, and command line access (the daemon's
//! HTTP API, [`ApiSwitch`](crate::daemon::ApiSwitch)). Features add their
//! own sections (clipboard: "Sync clipboard") through
//! [`settings_sections`](crate::ui::features::Features::settings_sections).

use std::path::Path;

use iced::{
    Background, Border, Element, Font, Length, Theme,
    widget::{button, column, container, row, scrollable, text},
};
use iced_fonts::lucide;

use crate::{
    client::{API_TOKEN_ENV, API_URL_ENV},
    core::SettingsSnapshot,
    daemon::ApiStatus,
    ui::{
        store::{Load, Store},
        widgets,
    },
};

/// Command line access as the page shows it.
#[derive(Clone, Copy, Default)]
pub struct CommandLine<'a> {
    /// The API now; `None` until it has been read.
    pub status: Option<&'a ApiStatus>,
    /// Where `ferry-cli` is, when it was installed next to the app.
    pub cli_path: Option<&'a Path>,
}

/// What a shell needs to reach the app's API: its address and token as
/// environment variables, in PowerShell's syntax on Windows. With `reveal`
/// off the token is masked, for showing. `None` while it doesn't listen.
pub fn cli_setup(status: &ApiStatus, reveal: bool) -> Option<String> {
    let address = status.address?;
    let token = status.token.as_ref()?;
    let token = if reveal {
        token.expose_secret().to_owned()
    } else {
        "•".repeat(12)
    };
    let url = format!("http://{address}");
    Some(if cfg!(windows) {
        format!("$env:{API_URL_ENV} = \"{url}\"\n$env:{API_TOKEN_ENV} = \"{token}\"")
    } else {
        format!("export {API_URL_ENV}={url}\nexport {API_TOKEN_ENV}={token}")
    })
}

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
    /// Turn command line access on or off.
    pub set_api_enabled: fn(bool) -> M,
    /// Copy [`cli_setup`], token and all.
    pub copy_cli_setup: M,
    /// Copy only the token.
    pub copy_api_token: M,
    /// Replace the token.
    pub new_api_token: M,
    /// Open the About page.
    pub about: M,
}

/// The settings `store` holds, with the features' `sections` of them
/// after the download folder, command line access, and a link to About.
/// `version` is the app's, and `start_on_login` whether the system starts
/// it at login.
pub fn view<'a, M: Clone + 'a>(
    store: &'a Store,
    sections: impl FnOnce(&'a SettingsSnapshot) -> Vec<Element<'a, M>>,
    version: &'a str,
    start_on_login: bool,
    cli: CommandLine<'a>,
    actions: Actions<M>,
) -> Element<'a, M> {
    let header = widgets::page_header("Settings", Some(actions.back.clone()), vec![]);
    let body = match store.settings() {
        Load::Loading => widgets::loading("Loading settings…"),
        Load::Failed(error) => widgets::error_view(error.as_str(), Some(actions.retry)),
        Load::Loaded(settings) => list(
            settings,
            sections(settings),
            version,
            start_on_login,
            cli,
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
    cli: CommandLine<'a>,
    actions: Actions<M>,
) -> Element<'a, M> {
    // Before `actions` gives its other messages away.
    let command_line = command_line(cli, &actions);
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
        .push(widgets::switch_setting(
            lucide::power,
            "Start when you log in",
            "Open in the tray, ready for your devices",
            start_on_login,
            actions.set_start_on_login,
        ))
        .push(command_line)
        .push(widgets::setting(
            lucide::info,
            "About Ferry",
            format!("Version {version}"),
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

/// The switch, and while it is on, how to set up `ferry-cli`: what to paste
/// into a shell, with buttons to copy it or the token, or why the API isn't
/// listening.
fn command_line<'a, M: Clone + 'a>(cli: CommandLine<'a>, actions: &Actions<M>) -> Element<'a, M> {
    let status = cli.status.filter(|status| status.enabled);
    let switch = widgets::switch_setting(
        lucide::square_terminal,
        "Command line access",
        "Let ferry-cli control this app",
        status.is_some(),
        actions.set_api_enabled,
    );
    let Some(status) = status else {
        return switch;
    };
    let mut details = column![].spacing(10);
    if let Some(error) = &status.error {
        details = details.push(text(error).size(13).style(text::danger));
    }
    if let Some(setup) = cli_setup(status, false) {
        let code = container(text(setup).size(13).font(Font::MONOSPACE))
            .padding([8, 10])
            .width(Length::Fill)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style {
                    background: Some(Background::Color(palette.background.weak.color)),
                    text_color: Some(palette.background.weak.text),
                    border: Border::default().rounded(8),
                    ..container::Style::default()
                }
            });
        let buttons = row![
            button(text("Copy setup").size(13))
                .padding([6, 12])
                .style(widgets::tonal)
                .on_press(actions.copy_cli_setup.clone()),
            button(text("Copy token").size(13))
                .padding([6, 12])
                .style(widgets::outlined)
                .on_press(actions.copy_api_token.clone()),
            button(text("New token").size(13))
                .padding([6, 12])
                .style(widgets::outlined)
                .on_press(actions.new_api_token.clone()),
        ]
        .spacing(8);
        details = details
            .push(
                text(
                    "ferry-cli on this computer finds the app by itself. \
                     Elsewhere, such as a script run as another user, paste this \
                     into its shell first:",
                )
                .size(13)
                .style(text::secondary),
            )
            .push(code)
            .push(buttons);
    }
    if let Some(path) = cli.cli_path {
        details = details.push(
            column![
                text("ferry-cli is installed at")
                    .size(13)
                    .style(text::secondary),
                widgets::selectable_text(&path.display().to_string()),
            ]
            .spacing(2),
        );
    }
    column![switch, widgets::card(details)].spacing(6).into()
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
        Api(bool),
        CopySetup,
        CopyToken,
        NewToken,
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
            set_api_enabled: Message::Api,
            copy_cli_setup: Message::CopySetup,
            copy_api_token: Message::CopyToken,
            new_api_token: Message::NewToken,
            about: Message::About,
        }
    }

    fn listening() -> ApiStatus {
        ApiStatus {
            enabled: true,
            switchable: true,
            port: 24_816,
            address: Some("127.0.0.1:24816".parse().unwrap()),
            token: Some(crate::config::ApiToken::from_secret("s3cret-token").unwrap()),
            error: None,
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
        let status = listening();
        let cli = CommandLine {
            status: Some(&status),
            cli_path: None,
        };
        let mut ui = Simulator::new(view(store, sections, "1.2.3 (dev)", false, cli, actions()));
        ui.click(target).unwrap();
        ui.into_messages().collect()
    }

    #[test]
    fn every_setting_is_shown_with_its_value() {
        let store = store();
        let mut ui = Simulator::new(view(
            &store,
            sections,
            "1.2.3 (dev)",
            false,
            CommandLine::default(),
            actions(),
        ));
        for shown in [
            "Device name",
            "Desktop",
            "Save received files in",
            "/home/me/Downloads",
            "Sync clipboard",
            "Keep running when the window is closed",
            "Start when you log in",
            "Command line access",
            "About Ferry",
            "Version 1.2.3 (dev)",
        ] {
            assert!(ui.find(shown).is_ok(), "{shown}");
        }
        assert!(
            ui.find("Copy setup").is_err(),
            "nothing to set up while off"
        );
    }

    #[test]
    fn command_line_access_shows_the_setup_with_the_token_masked() {
        let store = store();
        let status = listening();
        let path = Path::new("/Applications/Ferry.app/Contents/MacOS/ferry-cli");
        let mut ui = Simulator::new(view(
            &store,
            sections,
            "",
            false,
            CommandLine {
                status: Some(&status),
                cli_path: Some(path),
            },
            actions(),
        ));
        let shown = cli_setup(&status, false).unwrap();
        assert!(ui.find(shown.as_str()).is_ok());
        assert!(!shown.contains("s3cret-token"));
        assert!(shown.contains("http://127.0.0.1:24816"));
        assert!(ui.find("ferry-cli is installed at").is_ok());

        let copied = cli_setup(&status, true).unwrap();
        assert!(copied.contains("FERRY_API_TOKEN") && copied.contains("s3cret-token"));
        assert!(copied.contains("FERRY_API_URL"));

        let broken = ApiStatus {
            address: None,
            error: Some("couldn’t listen on port 24816".into()),
            ..listening()
        };
        let mut ui = Simulator::new(view(
            &store,
            sections,
            "",
            false,
            CommandLine {
                status: Some(&broken),
                cli_path: None,
            },
            actions(),
        ));
        assert!(ui.find("couldn’t listen on port 24816").is_ok());
        assert!(ui.find("Copy setup").is_err());
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
        assert_eq!(
            clicked(&store, "Command line access"),
            [Message::Api(false)]
        );
        assert_eq!(clicked(&store, "Copy setup"), [Message::CopySetup]);
        assert_eq!(clicked(&store, "Copy token"), [Message::CopyToken]);
        assert_eq!(clicked(&store, "New token"), [Message::NewToken]);
        assert_eq!(clicked(&store, "About Ferry"), [Message::About]);
        assert_eq!(
            clicked(&store, iced::widget::Id::from("Back")),
            [Message::Back]
        );
    }

    #[test]
    fn loading_and_failure_have_their_own_views() {
        let loading = Store::default();
        let mut ui = Simulator::new(view(
            &loading,
            sections,
            "",
            false,
            CommandLine::default(),
            actions(),
        ));
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
            view(
                &store,
                sections,
                "v1.2.0",
                false,
                CommandLine::default(),
                actions(),
            )
        });
        let status = listening();
        testing::snapshot("settings-command-line", (440.0, 900.0), || {
            view(
                &store,
                sections,
                "v1.2.0",
                false,
                CommandLine {
                    status: Some(&status),
                    cli_path: Some(Path::new("/usr/bin/ferry-cli")),
                },
                actions(),
            )
        });
    }
}
