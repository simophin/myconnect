//! What features ask of the shell: the things they share, which the shell
//! owns (toasts, reports, notifications, navigation, dialogs, pickers).
//! Each is a [`Message`] the shell handles; `origin` is where the action
//! that asked came from, which decides how it shows (`App::react`).

use std::{fmt, ops::Range, path::PathBuf};

use iced::Task;

use crate::ui::{
    Message, Origin,
    features::{Callback, Feature},
    overlay::dialog::Validator,
    route::Route,
};

/// Ask for a line of text. `validate` gives the error to show under the
/// field, if any; `then` gets the text once it is valid.
#[derive(Clone)]
pub(crate) struct Prompt {
    pub title: String,
    pub label: String,
    pub initial: String,
    /// The characters of `initial` selected when the dialog opens (a
    /// file's name without its extension); otherwise the cursor is at the
    /// end.
    pub selection: Option<Range<usize>>,
    pub confirm_label: String,
    pub validate: Validator,
    pub then: Callback<String>,
    pub origin: Origin,
}

impl fmt::Debug for Prompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Prompt")
            .field("title", &self.title)
            .field("initial", &self.initial)
            .field("origin", &self.origin)
            .finish_non_exhaustive()
    }
}

/// Pick files to open; `then` gets them, and isn't called on cancel.
#[derive(Clone)]
pub(crate) struct PickFiles {
    pub title: String,
    pub then: Callback<Vec<PathBuf>>,
    pub origin: Origin,
}

impl fmt::Debug for PickFiles {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PickFiles")
            .field("title", &self.title)
            .field("origin", &self.origin)
            .finish_non_exhaustive()
    }
}

/// A toast with no button.
pub(crate) fn toast(origin: Origin, text: impl Into<String>) -> Task<Message> {
    Task::done(Message::Toast {
        text: text.into(),
        action: None,
        origin,
    })
}

/// An action went well: `text` says how.
pub(crate) fn done(origin: Origin, text: impl Into<String>) -> Task<Message> {
    Task::done(Message::Report {
        text: text.into(),
        failure: None,
        origin,
    })
}

/// An action failed: `title` says which ("Couldn’t ping Pixel"), `text`
/// why.
pub(crate) fn failed(
    origin: Origin,
    title: impl Into<String>,
    text: impl Into<String>,
) -> Task<Message> {
    Task::done(Message::Report {
        text: text.into(),
        failure: Some(title.into()),
        origin,
    })
}

/// A toast while the window is focused, a desktop notification otherwise.
pub(crate) fn notify(title: impl Into<String>, body: impl Into<String>) -> Task<Message> {
    Task::done(Message::Notify {
        title: title.into(),
        body: body.into(),
    })
}

pub(crate) fn navigate(origin: Origin, route: Route) -> Task<Message> {
    Task::done(Message::Navigate(route, origin))
}

/// Ask before doing something; `then` is sent on confirm.
pub(crate) fn confirm(
    origin: Origin,
    title: impl Into<String>,
    body: impl Into<String>,
    confirm_label: impl Into<String>,
    then: Feature,
) -> Task<Message> {
    Task::done(Message::Confirm {
        title: title.into(),
        body: body.into(),
        confirm_label: confirm_label.into(),
        then,
        origin,
    })
}

pub(crate) fn prompt(prompt: Prompt) -> Task<Message> {
    Task::done(Message::Prompt(Box::new(prompt)))
}

/// Pick files to open; `then` gets them, and isn't called on cancel.
pub(crate) fn pick_files(
    origin: Origin,
    title: impl Into<String>,
    then: Callback<Vec<PathBuf>>,
) -> Task<Message> {
    Task::done(Message::PickFiles(PickFiles {
        title: title.into(),
        then,
        origin,
    }))
}
