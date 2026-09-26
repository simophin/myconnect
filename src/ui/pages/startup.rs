//! What the window shows while the daemon starts, and if it can't.

use iced::Element;

use crate::ui::{i18n::fl, widgets};

pub fn starting<'a, M: 'a>() -> Element<'a, M> {
    widgets::loading(fl!("startup-starting"))
}

/// The daemon didn't start: why, and Retry.
pub fn failed<M: Clone + 'static>(error: &str, retry: M) -> Element<'_, M> {
    widgets::error_view(fl!("startup-failed", error = error), Some(retry))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing;

    #[test]
    fn snapshot_startup() {
        testing::snapshot("startup-starting", (440.0, 400.0), starting::<()>);
        testing::snapshot("startup-failed", (440.0, 400.0), || {
            failed(
                "could not load the local identity: Permission denied (os error 13)",
                (),
            )
        });
    }
}
