//! Toasts: short messages stacked at the bottom of the window, gone after a
//! few seconds, with an optional button that goes somewhere. The Flutter
//! app's snackbar.

use std::time::Duration;

use iced::{
    Alignment, Background, Border, Element, Length, Theme,
    widget::{button, column, container, row, text},
};

use crate::ui::route::Route;

/// How long a toast stays up.
pub const DURATION: Duration = Duration::from_secs(4);

/// The most toasts shown at once; older ones go first.
const MAX: usize = 3;

#[derive(Debug, Clone)]
pub struct Toast {
    pub id: u64,
    pub text: String,
    /// A button's label and where it goes.
    pub action: Option<(String, Route)>,
}

/// The toasts showing, oldest first.
#[derive(Debug, Default)]
pub struct Toasts {
    items: Vec<Toast>,
    next_id: u64,
}

impl Toasts {
    /// Show a toast; returns its id, to dismiss it later.
    pub fn push(&mut self, text: String, action: Option<(String, Route)>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Toast { id, text, action });
        if self.items.len() > MAX {
            self.items.remove(0);
        }
        id
    }

    pub fn dismiss(&mut self, id: u64) {
        self.items.retain(|toast| toast.id != id);
    }

    pub fn items(&self) -> &[Toast] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The toasts at the bottom of the window. `on_action` makes the message
    /// for a toast's button, from the toast's id and route.
    pub fn view<'a, M: Clone + 'a>(&'a self, on_action: fn(u64, Route) -> M) -> Element<'a, M> {
        let toasts = self.items.iter().map(|toast| {
            let mut content = row![text(&toast.text).size(14).width(Length::Fill)]
                .spacing(12)
                .align_y(Alignment::Center);
            if let Some((label, route)) = &toast.action {
                content = content.push(
                    button(text(label).size(14))
                        .style(|theme: &Theme, status| {
                            let palette = theme.extended_palette();
                            button::Style {
                                text_color: palette.primary.weak.color,
                                ..button::text(theme, status)
                            }
                        })
                        .on_press(on_action(toast.id, route.clone())),
                );
            }
            container(content)
                .padding([10, 16])
                .max_width(400)
                .style(|theme: &Theme| {
                    let palette = theme.extended_palette();
                    // Inverted, so it stands out on either theme.
                    container::Style {
                        background: Some(Background::Color(palette.background.base.text)),
                        text_color: Some(palette.background.base.color),
                        border: Border::default().rounded(8),
                        ..container::Style::default()
                    }
                })
                .into()
        });
        container(column(toasts).spacing(8).align_x(Alignment::Center))
            .padding(16)
            .center_x(Length::Fill)
            .align_bottom(Length::Fill)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing;

    #[test]
    fn only_the_newest_few_show() {
        let mut toasts = Toasts::default();
        let first = toasts.push("1".into(), None);
        for text in ["2", "3", "4"] {
            toasts.push(text.into(), None);
        }
        let texts: Vec<_> = toasts.items().iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["2", "3", "4"]);
        // Dismissing one that already went is harmless.
        toasts.dismiss(first);
        assert_eq!(toasts.items().len(), 3);
    }

    #[test]
    fn snapshot_toasts() {
        let mut toasts = Toasts::default();
        toasts.push("Pinged Pixel 8a.".into(), None);
        toasts.push(
            "Downloading holiday.jpg".into(),
            Some(("Transfers".into(), Route::Transfers)),
        );
        testing::snapshot("toasts", (440.0, 300.0), || toasts.view(|_, route| route));
    }
}
