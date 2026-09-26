//! Modal dialogs: a question to confirm, or a line of text to type.
//!
//! One shows at a time; others wait their turn. Enter submits, Escape or a
//! click outside cancels. A dialog either closes as soon as it is
//! submitted ([`Submit::Close`], what features get through `ui::shell`),
//! or stays open and busy while its work runs and shows the work's error
//! under the field ([`Submit::Run`], for the shell's rename and add by IP).

use std::{collections::VecDeque, fmt, ops::Range, sync::Arc};

use iced::{
    Alignment, Background, Border, Color, Element, Length, Task, Theme,
    widget::{
        self, Id, button, center, column, container, mouse_area, opaque, row, space, text,
        text_input,
    },
};

use crate::ui::{i18n::fl, widgets::bold};

/// Checks a typed value: the error to show under the field, if any.
pub type Validator = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Makes the message a dialog sends from what it got (the field's text).
pub type Callback<A, M> = Arc<dyn Fn(A) -> M + Send + Sync>;

/// The dialog's text field, for focusing it.
const FIELD: &str = "dialog-field";

/// What the user did in the dialog.
#[derive(Debug, Clone)]
pub enum DialogEvent {
    Input(String),
    Submit,
    Cancel,
}

/// A line of text the dialog asks for.
#[derive(Clone, Default)]
pub struct Field {
    pub value: String,
    pub label: Option<String>,
    /// Placeholder text while the field is empty.
    pub hint: Option<String>,
    /// A line under the field while there is no error.
    pub helper: Option<String>,
    /// Characters allowed, shown as a counter.
    pub max_len: Option<usize>,
    /// Checked on submit: the error to show, if any.
    pub validate: Option<Validator>,
    /// The characters of `value` selected when the dialog opens; without
    /// it, the cursor is at the end.
    pub selection: Option<Range<usize>>,
}

/// Work that runs while the dialog stays open: `Err` is shown in the
/// dialog, `Ok` closes it and sends its message (the work's answer, for
/// its owner to apply).
pub type Work<M> = Arc<dyn Fn(String) -> Task<Result<M, String>> + Send + Sync>;

/// What submitting does.
#[derive(Clone)]
pub enum Submit<M> {
    /// Close the dialog and send the message made from the field's text
    /// (empty for a confirmation).
    Close(Callback<String, M>),
    /// Keep the dialog open, busy, while the work runs.
    Run(Work<M>),
}

#[derive(Clone)]
pub struct Dialog<M> {
    pub title: String,
    pub body: Option<String>,
    pub field: Option<Field>,
    pub confirm_label: String,
    pub submit: Submit<M>,
    /// Destructive: the confirm button says so.
    pub danger: bool,
    busy: bool,
    error: Option<String>,
}

impl<M> Dialog<M> {
    /// Ask before doing something.
    pub fn confirm(
        title: impl Into<String>,
        body: impl Into<String>,
        confirm_label: impl Into<String>,
        submit: Submit<M>,
    ) -> Self {
        Self {
            title: title.into(),
            body: Some(body.into()),
            field: None,
            confirm_label: confirm_label.into(),
            submit,
            danger: false,
            busy: false,
            error: None,
        }
    }

    /// Ask for a line of text.
    pub fn prompt(
        title: impl Into<String>,
        field: Field,
        confirm_label: impl Into<String>,
        submit: Submit<M>,
    ) -> Self {
        Self {
            title: title.into(),
            body: None,
            field: Some(field),
            confirm_label: confirm_label.into(),
            submit,
            danger: false,
            busy: false,
            error: None,
        }
    }

    /// Text between the title and the field. Names the user or a device
    /// chose (a file, a device, a notification) belong here, not in the
    /// title: they can be any length, and here they wrap.
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn value(&self) -> &str {
        self.field.as_ref().map_or("", |field| &field.value)
    }
}

impl<M> fmt::Debug for Dialog<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dialog")
            .field("title", &self.title)
            .field("busy", &self.busy)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

/// What the shell must do after a [`DialogEvent`].
pub enum Step<M> {
    Nothing,
    /// The dialog closed with this message for its owner.
    Send(M),
    /// The dialog's work, which reports back through [`Dialogs::finished`]
    /// with this dialog id.
    Run(u64, Task<Result<M, String>>),
}

/// The dialog showing and the ones waiting, oldest first.
pub struct Dialogs<M> {
    queue: VecDeque<(u64, Dialog<M>)>,
    next_id: u64,
}

impl<M> Default for Dialogs<M> {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            next_id: 0,
        }
    }
}

impl<M: Clone + 'static> Dialogs<M> {
    /// Show `dialog`, or queue it behind the one showing.
    pub fn open<T: Send + 'static>(&mut self, dialog: Dialog<M>) -> Task<T> {
        let id = self.next_id;
        self.next_id += 1;
        self.queue.push_back((id, dialog));
        if self.queue.len() == 1 {
            self.focus()
        } else {
            Task::none()
        }
    }

    /// The dialog showing.
    pub fn current(&self) -> Option<&Dialog<M>> {
        self.queue.front().map(|(_, dialog)| dialog)
    }

    pub fn update(&mut self, event: DialogEvent) -> Step<M> {
        let Some((id, dialog)) = self.queue.front_mut() else {
            return Step::Nothing;
        };
        match event {
            DialogEvent::Input(value) => {
                if let Some(field) = &mut dialog.field
                    && !dialog.busy
                {
                    field.value = match field.max_len {
                        Some(max) => value.chars().take(max).collect(),
                        None => value,
                    };
                }
                Step::Nothing
            }
            DialogEvent::Cancel => {
                self.queue.pop_front();
                Step::Nothing
            }
            DialogEvent::Submit => {
                if dialog.busy {
                    return Step::Nothing;
                }
                let value = dialog.value().to_owned();
                if let Some(error) = dialog
                    .field
                    .as_ref()
                    .and_then(|field| field.validate.as_ref())
                    .and_then(|validate| validate(&value))
                {
                    dialog.error = Some(error);
                    return Step::Nothing;
                }
                match dialog.submit.clone() {
                    Submit::Close(then) => {
                        self.queue.pop_front();
                        Step::Send(then(value))
                    }
                    Submit::Run(work) => {
                        dialog.busy = true;
                        dialog.error = None;
                        Step::Run(*id, work(value))
                    }
                }
            }
        }
    }

    /// The work of dialog `id` finished: on success the dialog closes and
    /// the work's message is returned. Nothing happens if the dialog was
    /// cancelled meanwhile.
    pub fn finished(&mut self, id: u64, result: Result<M, String>) -> Option<M> {
        let index = self.queue.iter().position(|(queued, _)| *queued == id)?;
        match result {
            Ok(message) => {
                self.queue.remove(index);
                Some(message)
            }
            Err(error) => {
                let dialog = &mut self.queue[index].1;
                dialog.busy = false;
                dialog.error = Some(error);
                None
            }
        }
    }

    /// Focus the showing dialog's field, if it has one.
    pub fn focus<T: Send + 'static>(&self) -> Task<T> {
        match self.current() {
            Some(Dialog {
                field: Some(field), ..
            }) => Task::batch([
                widget::operation::focus(FIELD),
                match &field.selection {
                    Some(range) => widget::operation::select_range(FIELD, range.start, range.end),
                    None => widget::operation::move_cursor_to_end(FIELD),
                },
            ]),
            _ => Task::none(),
        }
    }

    /// `base` with the showing dialog over it.
    pub fn view<'a, T: Clone + 'a>(
        &'a self,
        base: Element<'a, T>,
        on_event: fn(DialogEvent) -> T,
    ) -> Element<'a, T> {
        match self.current() {
            Some(dialog) => modal(
                base,
                view(dialog).map(on_event),
                Some(on_event(DialogEvent::Cancel)),
            ),
            None => base,
        }
    }
}

/// `content` centred over `base`, which is dimmed and ignores the mouse;
/// a click outside `content` sends `on_blur`, if given. As in iced's
/// `modal` example.
pub fn modal<'a, T: Clone + 'a>(
    base: Element<'a, T>,
    content: Element<'a, T>,
    on_blur: Option<T>,
) -> Element<'a, T> {
    let backdrop = mouse_area(center(opaque(content)).style(|_theme| container::Style {
        background: Some(Background::Color(Color {
            a: 0.5,
            ..Color::BLACK
        })),
        ..container::Style::default()
    }));
    let backdrop = match on_blur {
        Some(on_blur) => backdrop.on_press(on_blur),
        None => backdrop,
    };
    iced::widget::stack![base, opaque(backdrop)].into()
}

fn view<M>(dialog: &Dialog<M>) -> Element<'_, DialogEvent> {
    let mut content = column![text(&dialog.title).size(20).font(bold())].spacing(16);
    if let Some(body) = &dialog.body {
        content = content.push(text(body).size(14).wrapping(text::Wrapping::WordOrGlyph));
    }
    if let Some(field) = &dialog.field {
        content = content.push(field_view(field, dialog.error(), dialog.busy));
    } else if let Some(error) = dialog.error() {
        content = content.push(text(error).size(13).style(text::danger));
    }

    let confirm = button(text(&dialog.confirm_label))
        .padding([8, 18])
        .style(if dialog.danger {
            button::danger
        } else {
            button::primary
        })
        .on_press_maybe((!dialog.busy).then_some(DialogEvent::Submit));
    content = content.push(
        row![
            space::horizontal(),
            button(text(fl!("dialog-cancel")))
                .padding([8, 14])
                .style(button::text)
                .on_press(DialogEvent::Cancel),
            confirm,
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );

    surface(content)
}

/// A dialog's card, for content drawn over the page.
///
/// It has a border rather than a shadow: iced's software renderer draws
/// shadows unclipped, so each partial redraw (a blinking cursor, an
/// activity bar) darkens them further until the card turns black.
pub fn surface<'a, T: 'a>(content: impl Into<Element<'a, T>>) -> Element<'a, T> {
    container(content)
        .padding(24)
        .width(Length::Fill)
        .max_width(400)
        .style(surface_style)
        .into()
}

/// The dialog card's look, for dialogs of other sizes.
pub fn surface_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        text_color: Some(palette.background.base.text),
        border: Border::default()
            .rounded(16)
            .width(1)
            .color(palette.background.strong.color),
        ..container::Style::default()
    }
}

fn field_view<'a>(
    field: &'a Field,
    error: Option<&'a str>,
    busy: bool,
) -> Element<'a, DialogEvent> {
    let mut input = text_input(field.hint.as_deref().unwrap_or(""), &field.value)
        .id(Id::new(FIELD))
        .padding(10)
        .style(move |theme: &Theme, status| {
            let style = text_input::default(theme, status);
            if error.is_some() {
                let danger = theme.extended_palette().danger.base.color;
                text_input::Style {
                    border: style.border.color(danger),
                    ..style
                }
            } else {
                style
            }
        });
    if !busy {
        input = input
            .on_input(DialogEvent::Input)
            .on_submit(DialogEvent::Submit);
    }

    let mut column = column![].spacing(6);
    if let Some(label) = &field.label {
        column = column.push(text(label).size(13).style(text::secondary));
    }
    column = column.push(input);

    let note = match (error, &field.helper) {
        (Some(error), _) => Some(text(error).size(12).style(text::danger)),
        (None, Some(helper)) => Some(text(helper).size(12).style(text::secondary)),
        (None, None) => None,
    };
    let counter = field.max_len.map(|max| {
        text(fl!(
            "dialog-counter",
            count = field.value.chars().count(),
            max = max
        ))
        .size(12)
        .style(text::secondary)
    });
    if note.is_some() || counter.is_some() {
        let mut under = row![].spacing(8);
        under = under.push(
            container(note.map_or_else(|| space::horizontal().into(), Element::from))
                .width(Length::Fill),
        );
        if let Some(counter) = counter {
            under = under.push(counter);
        }
        column = column.push(under);
    }
    column.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing;

    fn name_field() -> Field {
        Field {
            value: "Desk".into(),
            helper: Some("How this computer appears on your other devices".into()),
            max_len: Some(32),
            validate: Some(Arc::new(|name: &str| {
                name.trim().is_empty().then(|| "Enter a name.".to_owned())
            })),
            ..Field::default()
        }
    }

    fn closing(title: &str) -> Dialog<String> {
        Dialog::prompt(
            title,
            name_field(),
            "Save",
            Submit::Close(Arc::new(|name| format!("saved {name}"))),
        )
    }

    #[test]
    fn a_closing_prompt_validates_then_sends_the_text() {
        let mut dialogs = Dialogs::default();
        let _: Task<()> = dialogs.open(closing("Name"));

        dialogs.update(DialogEvent::Input("  ".into()));
        assert!(matches!(dialogs.update(DialogEvent::Submit), Step::Nothing));
        assert_eq!(dialogs.current().unwrap().error(), Some("Enter a name."));

        dialogs.update(DialogEvent::Input("Laptop".into()));
        let Step::Send(message) = dialogs.update(DialogEvent::Submit) else {
            panic!("the dialog should close with its message");
        };
        assert_eq!(message, "saved Laptop");
        assert!(dialogs.current().is_none());
    }

    #[test]
    fn the_field_stops_at_its_limit() {
        let mut dialogs = Dialogs::default();
        let _: Task<()> = dialogs.open(closing("Name"));
        dialogs.update(DialogEvent::Input("x".repeat(40)));
        assert_eq!(dialogs.current().unwrap().value().len(), 32);
    }

    #[test]
    fn running_work_keeps_the_dialog_busy_until_it_finishes() {
        let mut dialogs: Dialogs<String> = Dialogs::default();
        let _: Task<()> = dialogs.open(Dialog::prompt(
            "Add by IP address",
            Field::default(),
            "Add",
            Submit::Run(Arc::new(|_| Task::none())),
        ));
        dialogs.update(DialogEvent::Input("300.1.1.1".into()));
        let Step::Run(id, _) = dialogs.update(DialogEvent::Submit) else {
            panic!("the dialog should run its work");
        };
        let dialog = dialogs.current().unwrap();
        assert!(dialog.is_busy());
        // Busy: typing and submitting again do nothing.
        dialogs.update(DialogEvent::Input("typed while busy".into()));
        assert!(matches!(dialogs.update(DialogEvent::Submit), Step::Nothing));
        assert_eq!(dialogs.current().unwrap().value(), "300.1.1.1");

        dialogs.finished(id, Err("Enter an IPv4 address, like 192.168.1.20.".into()));
        let dialog = dialogs.current().unwrap();
        assert!(!dialog.is_busy());
        assert_eq!(
            dialog.error(),
            Some("Enter an IPv4 address, like 192.168.1.20.")
        );

        let Step::Run(id, _) = dialogs.update(DialogEvent::Submit) else {
            panic!("the dialog should run its work again");
        };
        assert_eq!(dialogs.current().unwrap().error(), None);
        assert_eq!(
            dialogs.finished(id, Ok("added".into())).as_deref(),
            Some("added"),
            "the work's message is sent"
        );
        assert!(dialogs.current().is_none());
    }

    #[test]
    fn work_finishing_after_cancel_is_ignored() {
        let mut dialogs: Dialogs<String> = Dialogs::default();
        let _: Task<()> = dialogs.open(Dialog::prompt(
            "Add",
            Field::default(),
            "Add",
            Submit::Run(Arc::new(|_| Task::none())),
        ));
        let Step::Run(id, _) = dialogs.update(DialogEvent::Submit) else {
            panic!("the dialog should run its work");
        };
        dialogs.update(DialogEvent::Cancel);
        let _: Task<()> = dialogs.open(closing("Next"));
        assert_eq!(dialogs.finished(id, Err("late".into())), None);
        let dialog = dialogs.current().unwrap();
        assert_eq!(dialog.title, "Next");
        assert_eq!(dialog.error(), None);
    }

    #[test]
    fn dialogs_show_one_at_a_time() {
        let mut dialogs = Dialogs::default();
        let _: Task<()> = dialogs.open(closing("First"));
        let _: Task<()> = dialogs.open(closing("Second"));
        assert_eq!(dialogs.current().unwrap().title, "First");
        dialogs.update(DialogEvent::Cancel);
        assert_eq!(dialogs.current().unwrap().title, "Second");
        dialogs.update(DialogEvent::Cancel);
        assert!(dialogs.current().is_none());
    }

    #[test]
    fn snapshot_dialogs() {
        let base = || -> Element<'static, DialogEvent> {
            container(text("The page underneath"))
                .center(Length::Fill)
                .into()
        };

        let mut confirm: Dialogs<()> = Dialogs::default();
        let _: Task<()> = confirm.open(
            Dialog::confirm(
                "Unpair device?",
                "Pixel 8a will need to be paired again before it can exchange anything \
                 with this computer.",
                "Unpair",
                Submit::Close(Arc::new(|_| ())),
            )
            .danger(),
        );
        testing::snapshot("dialog-confirm", (440.0, 400.0), || {
            confirm.view(base(), |event| event)
        });

        let mut prompt = Dialogs::default();
        let _: Task<()> = prompt.open(closing("Device name"));
        prompt.update(DialogEvent::Input(" ".into()));
        prompt.update(DialogEvent::Submit);
        testing::snapshot("dialog-prompt-error", (440.0, 400.0), || {
            prompt.view(base(), |event| event)
        });
    }
}
