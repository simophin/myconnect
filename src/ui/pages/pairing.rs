//! One outgoing pairing request: the verification code while the other
//! device decides, then the outcome.

use iced::{
    Alignment, Element, Length, Theme,
    widget::{button, column, container, row, text},
};
use iced_fonts::lucide;
use uuid::Uuid;

use crate::{
    core::{PairingSnapshot, PairingStatus},
    ui::{activity::activity_bar, plugin::Icon, route::Route, store::Store, widgets},
};

/// The messages the page sends.
pub struct Actions<M> {
    pub navigate: fn(Route) -> M,
    /// Cancel the pairing of this id.
    pub cancel: fn(Uuid) -> M,
    /// Start a new pairing with the device of this id.
    pub retry: fn(String) -> M,
}

/// The page of pairing `pairing_id`, from what `store` holds. While `busy`
/// (a cancel or a new start runs), Cancel and Try again are disabled.
pub fn view<'a, M: Clone + 'a>(
    store: &'a Store,
    pairing_id: Uuid,
    busy: bool,
    actions: Actions<M>,
) -> Element<'a, M> {
    let header = widgets::page_header(
        "Pairing",
        Some((actions.navigate)(Route::AddDevice)),
        vec![],
    );
    let body: Element<'a, M> = match store.pairing(pairing_id) {
        None => text("This pairing request no longer exists.").into(),
        Some(pairing) => content(pairing, busy, &actions),
    };
    widgets::page(
        header,
        container(container(body).max_width(420).padding(24)).center_x(Length::Fill),
    )
}

/// What the page says about a pairing in `status` with the device `name`:
/// its icon (none while pending), title and detail.
pub fn describe(status: PairingStatus, name: &str) -> (Option<Icon>, String, Option<String>) {
    match status {
        PairingStatus::Requested | PairingStatus::AwaitingConfirmation => (
            None,
            format!("Waiting for {name}"),
            Some(format!(
                "Check that {name} shows the same code, then accept the request there."
            )),
        ),
        PairingStatus::Accepted => (
            Some(lucide::circle_check),
            format!("Paired with {name}"),
            None,
        ),
        PairingStatus::Rejected => (
            Some(lucide::ban),
            "Pairing declined".into(),
            Some("The request was declined or cancelled.".into()),
        ),
        PairingStatus::Expired => (
            Some(lucide::timer_off),
            "Request timed out".into(),
            Some(format!("{name} did not answer in time.")),
        ),
        PairingStatus::Failed => (
            Some(lucide::circle_alert),
            "Pairing failed".into(),
            Some(format!("The connection to {name} was lost.")),
        ),
    }
}

fn content<'a, M: Clone + 'a>(
    pairing: &'a PairingSnapshot,
    busy: bool,
    actions: &Actions<M>,
) -> Element<'a, M> {
    let (icon, title, detail) = describe(pairing.status, &pairing.device_name);
    let pending = !pairing.status.is_terminal();

    let top: Element<'a, M> = match icon {
        Some(icon) => icon()
            .size(56)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.extended_palette().primary.base.color),
            })
            .into(),
        None => container(activity_bar(160, 4.0)).padding([26, 0]).into(),
    };
    let mut content = column![top, text(title).size(20).font(widgets::semibold())]
        .spacing(16)
        .align_x(Alignment::Center);
    if let Some(detail) = detail {
        content = content.push(text(detail).center());
    }
    if pending && let Some(code) = &pairing.verification_code {
        content = content.push(widgets::verification_code(code));
    }

    let labelled = |label: &'a str, style: fn(&Theme, button::Status) -> button::Style| {
        button(text(label)).padding([8, 18]).style(style)
    };
    let buttons = match pairing.status {
        _ if pending => row![
            labelled("Cancel", widgets::outlined)
                .on_press_maybe((!busy).then(|| (actions.cancel)(pairing.id)))
        ],
        PairingStatus::Accepted => row![
            labelled("Done", widgets::filled)
                .on_press((actions.navigate)(Route::Device(pairing.device_id.clone())))
        ],
        _ => row![
            labelled("Close", widgets::outlined).on_press((actions.navigate)(Route::AddDevice)),
            labelled("Try again", widgets::filled)
                .on_press_maybe((!busy).then(|| (actions.retry)(pairing.device_id.clone()))),
        ],
    };
    content =
        content.push(container(buttons.spacing(12)).padding(iced::Padding::default().top(16)));
    content.into()
}

#[cfg(test)]
mod tests {
    use iced_test::simulator::Simulator;

    use super::*;
    use crate::{
        core::PairingDirection,
        ui::{store::Snapshot, testing},
    };

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Asked {
        Go(Route),
        Cancel(Uuid),
        Retry(String),
    }

    const PEER: &str = "740bd4b9b4184ee497d6caf1da8151be";

    fn pairing(status: PairingStatus) -> PairingSnapshot {
        PairingSnapshot {
            id: Uuid::from_u128(7),
            device_id: PEER.into(),
            device_name: "Pixel 8a".into(),
            direction: PairingDirection::Outgoing,
            status,
            verification_code: Some("A1B2C3D4".into()),
            created_at: 0,
            expires_at: 30_000,
            error_code: None,
        }
    }

    fn store(pairings: Vec<PairingSnapshot>) -> Store {
        let mut store = Store::default();
        store.apply_snapshot(Snapshot {
            devices: Ok(Vec::new()),
            pairings: Ok(pairings),
            transfers: Vec::new(),
            settings: Err(String::new()),
        });
        store
    }

    fn page(store: &Store, busy: bool) -> Element<'_, Asked> {
        view(
            store,
            Uuid::from_u128(7),
            busy,
            Actions {
                navigate: Asked::Go,
                cancel: Asked::Cancel,
                retry: Asked::Retry,
            },
        )
    }

    fn click(status: PairingStatus, busy: bool, target: &str) -> Vec<Asked> {
        let store = store(vec![pairing(status)]);
        let mut ui = Simulator::new(page(&store, busy));
        ui.click(target).expect("the target is on the page");
        ui.into_messages().collect()
    }

    #[test]
    fn every_status_has_its_title_detail_and_buttons() {
        for (status, shown, hidden) in [
            (
                PairingStatus::AwaitingConfirmation,
                &[
                    "Waiting for Pixel 8a",
                    "Check that Pixel 8a shows the same code, then accept the request there.",
                    "A1B2C3D4",
                    "Cancel",
                ][..],
                &["Done", "Try again"][..],
            ),
            (
                PairingStatus::Accepted,
                &["Paired with Pixel 8a", "Done"],
                &["A1B2C3D4", "Cancel", "Try again"],
            ),
            (
                PairingStatus::Rejected,
                &[
                    "Pairing declined",
                    "The request was declined or cancelled.",
                    "Close",
                    "Try again",
                ],
                &["A1B2C3D4", "Done"],
            ),
            (
                PairingStatus::Expired,
                &[
                    "Request timed out",
                    "Pixel 8a did not answer in time.",
                    "Try again",
                ],
                &["Cancel"],
            ),
            (
                PairingStatus::Failed,
                &[
                    "Pairing failed",
                    "The connection to Pixel 8a was lost.",
                    "Try again",
                ],
                &["Cancel"],
            ),
        ] {
            let store = store(vec![pairing(status)]);
            let mut ui = Simulator::new(page(&store, false));
            for text in shown {
                assert!(ui.find(*text).is_ok(), "{status:?}: {text} is shown");
            }
            for text in hidden {
                assert!(ui.find(*text).is_err(), "{status:?}: {text} isn't shown");
            }
        }
    }

    #[test]
    fn a_pairing_that_is_gone_says_so() {
        let store = store(Vec::new());
        let mut ui = Simulator::new(page(&store, false));
        assert!(ui.find("This pairing request no longer exists.").is_ok());
    }

    #[test]
    fn the_buttons_cancel_retry_and_leave() {
        let id = Uuid::from_u128(7);
        assert_eq!(
            click(PairingStatus::Requested, false, "Cancel"),
            [Asked::Cancel(id)]
        );
        assert!(click(PairingStatus::Requested, true, "Cancel").is_empty());
        assert_eq!(
            click(PairingStatus::Accepted, false, "Done"),
            [Asked::Go(Route::Device(PEER.into()))]
        );
        assert_eq!(
            click(PairingStatus::Expired, false, "Close"),
            [Asked::Go(Route::AddDevice)]
        );
        assert_eq!(
            click(PairingStatus::Expired, false, "Try again"),
            [Asked::Retry(PEER.into())]
        );
        assert!(click(PairingStatus::Expired, true, "Try again").is_empty());
    }

    #[test]
    fn snapshot_pairing() {
        for (name, status) in [
            ("pairing-waiting", PairingStatus::AwaitingConfirmation),
            ("pairing-accepted", PairingStatus::Accepted),
            ("pairing-expired", PairingStatus::Expired),
        ] {
            let store = store(vec![pairing(status)]);
            testing::snapshot(name, (440.0, 520.0), || page(&store, false));
        }
    }
}
