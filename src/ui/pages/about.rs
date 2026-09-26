//! The About page: the app's icon, name and version, what it is, who made
//! it, and links to its source and to supporting its development. The
//! links open in the system browser.

use std::sync::LazyLock;

use iced::{
    Alignment, Element, Length,
    widget::{column, container, image, scrollable, text},
};
use iced_fonts::lucide;

use crate::ui::widgets;

pub const NAME: &str = "Ferry";
pub const DESCRIPTION: &str = "A KDE Connect client for macOS, Linux and Windows.";
pub const AUTHOR_URL: &str = "https://fanchao.dev";
pub const SOURCE_URL: &str = "https://github.com/simophin/ferryapp";
pub const SPONSOR_URL: &str = "https://github.com/sponsors/simophin";

/// The app's icon, the one its window has, decoded once.
static ICON: LazyLock<image::Handle> = LazyLock::new(|| {
    let png = include_bytes!("../../../assets/window_icon.png");
    let icon = ::image::load_from_memory(png)
        .expect("the app icon is a valid PNG")
        .into_rgba8();
    image::Handle::from_rgba(icon.width(), icon.height(), icon.into_raw())
});

/// What the page's controls ask for.
pub struct Actions<M> {
    pub back: M,
    /// Open a web page in the system browser.
    pub open_link: fn(&'static str) -> M,
}

/// The page, showing the app's `version`.
pub fn view<'a, M: Clone + 'a>(version: &'a str, actions: Actions<M>) -> Element<'a, M> {
    let header = widgets::page_header("About", Some(actions.back), vec![]);
    let app = column![
        image(ICON.clone()).width(96).height(96),
        text(NAME).size(28).font(widgets::bold()),
        text(format!("Version {version}"))
            .size(13)
            .style(text::secondary),
        widgets::gap(4),
        text(DESCRIPTION).size(15).center(),
    ]
    .spacing(6)
    .align_x(Alignment::Center);
    let link = |icon, title, detail, url| {
        let trailing = lucide::external_link().size(16).style(text::secondary);
        widgets::setting(
            icon,
            title,
            detail,
            Some(trailing.into()),
            Some((actions.open_link)(url)),
        )
    };
    let links = column![
        link(lucide::user, "Made by Fanchao", "fanchao.dev", AUTHOR_URL),
        link(
            lucide::code,
            "Source code",
            "github.com/simophin/ferryapp",
            SOURCE_URL,
        ),
        link(
            lucide::heart,
            "Support development",
            "Sponsor on GitHub",
            SPONSOR_URL,
        ),
    ]
    .spacing(8);
    let body = column![container(app).center_x(Length::Fill), links]
        .spacing(24)
        .max_width(480);
    let body = container(body).center_x(Length::Fill).padding([8, 0]);
    widgets::page(header, scrollable(body).spacing(6).height(Length::Fill))
}

#[cfg(test)]
mod tests {
    use iced_test::simulator::Simulator;

    use super::*;
    use crate::ui::testing;

    #[derive(Debug, Clone, PartialEq)]
    enum Message {
        Back,
        Open(&'static str),
    }

    fn actions() -> Actions<Message> {
        Actions {
            back: Message::Back,
            open_link: Message::Open,
        }
    }

    fn clicked<S>(target: S) -> Vec<Message>
    where
        S: iced_test::selector::Selector + Send,
        S::Output: iced_test::selector::Bounded + Clone + Send + Sync + 'static,
    {
        let mut ui = Simulator::new(view("1.2.3 (dev)", actions()));
        ui.click(target).unwrap();
        ui.into_messages().collect()
    }

    #[test]
    fn the_app_and_its_version_are_shown() {
        let mut ui = Simulator::new(view("1.2.3 (dev)", actions()));
        for shown in [NAME, "Version 1.2.3 (dev)", DESCRIPTION] {
            assert!(ui.find(shown).is_ok(), "{shown}");
        }
    }

    #[test]
    fn each_link_opens_its_page() {
        assert_eq!(clicked("Made by Fanchao"), [Message::Open(AUTHOR_URL)]);
        assert_eq!(clicked("Source code"), [Message::Open(SOURCE_URL)]);
        assert_eq!(clicked("Support development"), [Message::Open(SPONSOR_URL)]);
        assert_eq!(clicked(iced::widget::Id::from("Back")), [Message::Back]);
    }

    #[test]
    fn snapshot_about() {
        testing::snapshot("about", (440.0, 620.0), || {
            view("0.1.0 (v1.1.0-19-geeba428)", actions())
        });
    }
}
