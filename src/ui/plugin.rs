//! The seam between the UI core and each feature's UI half, which lives in
//! `src/plugins/<name>/ui.rs` and is listed in
//! `plugins::builtin_with_ui`. The UI core never names a feature.

/// A feature's UI half, as the shell stores it. For now it has only its id;
/// the slots a feature fills (device status, actions, pages, settings)
/// arrive as features move over to this UI.
pub trait ErasedUiPlugin {
    /// The same id as the feature's core plugin.
    fn id(&self) -> &'static str;
}
