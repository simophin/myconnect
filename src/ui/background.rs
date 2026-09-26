//! Keeping the app, and with it the daemon, running while the window is
//! closed: what the tray menu holds, and the notifications for what happens
//! meanwhile. The shell (`ui/mod.rs`) wires them to the window and the
//! desktop.

use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    core::{
        DeviceReachability, DeviceSnapshot, PairingSnapshot, TransferDirection, TransferStatus,
    },
    ui::{
        desktop::{
            notify::Notifier,
            tray::{TrayCommand, TrayItem},
        },
        plugin::ErasedUiPlugin,
        store::Store,
    },
};

/// The tray menu: Open, then each connected paired device with what can be
/// done to it (left out while devices are unknown, e.g. the daemon didn't
/// start), then Settings and Quit. The window lists the rest.
pub fn tray_menu(running: Option<(&Store, &[Box<dyn ErasedUiPlugin>])>) -> Vec<TrayItem> {
    let mut menu = vec![
        TrayItem::item("Open MyConnect", Some(TrayCommand::Open)),
        TrayItem::Separator,
    ];
    if let Some((store, plugins)) = running
        && let Some(paired) = store.paired_devices().into_loaded()
    {
        let connected: Vec<_> = paired
            .iter()
            .filter(|device| device.reachability == DeviceReachability::Connected)
            .collect();
        if paired.is_empty() {
            menu.push(TrayItem::item("No paired devices", None));
        } else if connected.is_empty() {
            menu.push(TrayItem::item("No devices connected", None));
        }
        menu.extend(
            connected
                .into_iter()
                .map(|device| device_menu(device, plugins)),
        );
        menu.push(TrayItem::Separator);
    }
    menu.extend([
        TrayItem::item("Settings", Some(TrayCommand::Settings)),
        TrayItem::Separator,
        TrayItem::item("Quit", Some(TrayCommand::Quit)),
    ]);
    menu
}

/// A device's submenu, "{name} · {status}", holding the plugins' actions
/// meant for the tray, then Show details.
fn device_menu(device: &DeviceSnapshot, plugins: &[Box<dyn ErasedUiPlugin>]) -> TrayItem {
    let status = plugins
        .iter()
        .find_map(|plugin| plugin.device_status(device));
    let label = match status {
        Some(status) => format!("{} · {}", device.device_name, status.label),
        None => device.device_name.clone(),
    };
    let mut items: Vec<_> = plugins
        .iter()
        .flat_map(|plugin| plugin.device_actions(device))
        .filter(|action| action.visible_in_tray)
        .map(|action| {
            let command = action.enabled.then(|| TrayCommand::Action(action.message));
            TrayItem::item(action.label, command)
        })
        .collect();
    items.extend([
        TrayItem::Separator,
        TrayItem::item(
            "Show details",
            Some(TrayCommand::ShowDevice(device.device_id.clone())),
        ),
    ]);
    TrayItem::Submenu { label, items }
}

/// The desktop notifications the shell shows, and which pairing request
/// each one is about.
#[derive(Default)]
pub struct Notifications {
    /// A notification per pending incoming request, or `None` when it
    /// arrived over a focused window and needed none.
    pairings: HashMap<Uuid, Option<u32>>,
    next_id: u32,
}

impl Notifications {
    /// Show a notification.
    pub fn show(&mut self, notifier: &dyn Notifier, title: &str, body: &str) -> u32 {
        self.next_id += 1;
        notifier.show(self.next_id, title, body);
        self.next_id
    }

    /// Follow the pending incoming pairing requests: notify about new ones
    /// unless the window is `focused` (its prompt is in front of the user),
    /// and withdraw the notification of each one resolved.
    pub fn pairings(
        &mut self,
        notifier: &dyn Notifier,
        pending: &[&PairingSnapshot],
        focused: bool,
    ) {
        self.pairings.retain(|id, notification| {
            let still = pending.iter().any(|pairing| pairing.id == *id);
            if !still && let Some(notification) = notification {
                notifier.withdraw(*notification);
            }
            still
        });
        for pairing in pending {
            if self.pairings.contains_key(&pairing.id) {
                continue;
            }
            let notification = (!focused).then(|| {
                self.next_id += 1;
                notifier.show(
                    self.next_id,
                    "Pairing request",
                    &format!("{} wants to pair with this computer.", pairing.device_name),
                );
                self.next_id
            });
            self.pairings.insert(pairing.id, notification);
        }
    }
}

/// Each transfer's status, to tell afterwards which ones finished.
pub fn transfer_statuses(store: &Store) -> HashMap<Uuid, TransferStatus> {
    store
        .transfers(None)
        .into_loaded()
        .unwrap_or_default()
        .into_iter()
        .map(|transfer| (transfer.id, transfer.status))
        .collect()
}

/// Files received since `before`: incoming transfers that completed. One
/// `before` didn't hold is skipped, so files received before the app
/// started aren't announced.
pub fn received_files(
    before: &HashMap<Uuid, TransferStatus>,
    store: &Store,
) -> Vec<(String, String)> {
    let mut received: Vec<_> = store
        .transfers(None)
        .into_loaded()
        .unwrap_or_default()
        .into_iter()
        .filter(|transfer| {
            transfer.direction == TransferDirection::Incoming
                && transfer.status == TransferStatus::Completed
                && before
                    .get(&transfer.id)
                    .is_some_and(|status| *status != TransferStatus::Completed)
        })
        .map(|transfer| (transfer.file_name.clone(), transfer.device_name.clone()))
        .collect();
    // Oldest first, as they finished.
    received.reverse();
    received
}
