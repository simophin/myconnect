//! `kdeconnect.notification` packet models.
//!
//! A peer (KDE Connect for Android) sends `kdeconnect.notification` for each
//! notification posted or updated on it, and again with `isCancel` when one
//! goes away. This device asks for all of them with
//! `kdeconnect.notification.request` (`{"request": true}`), dismisses one on
//! the peer with the same type (`{"cancel": id}`), answers one with
//! `kdeconnect.notification.reply`, and presses one of its buttons with
//! `kdeconnect.notification.action`.

use serde::{Deserialize, Serialize};
use serde_json::Number;

use crate::protocol::{BodyError, Packet};

/// A notification posted, updated or removed on the peer.
pub const PACKET_TYPE: &str = "kdeconnect.notification";
/// Ask for every current notification, or dismiss one.
pub const REQUEST_PACKET_TYPE: &str = "kdeconnect.notification.request";
/// Answer a notification that takes a reply.
pub const REPLY_PACKET_TYPE: &str = "kdeconnect.notification.reply";
/// Press one of a notification's buttons.
pub const ACTION_PACKET_TYPE: &str = "kdeconnect.notification.action";

/// Body of a `kdeconnect.notification` packet. Only `id` is always there;
/// a removal (`isCancel`) carries nothing else. Android sends `time` as a
/// string of milliseconds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationBody {
    pub id: String,
    #[serde(default)]
    pub is_cancel: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(default)]
    pub is_clearable: bool,
    /// Posted before this device asked (an answer to a request), so not
    /// news: shown, but not announced.
    #[serde(default)]
    pub silent: bool,
    /// Present when the notification takes a reply; the reply packet
    /// names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_reply_id: Option<String>,
    /// The notification's buttons, by label; `null` when it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<String>>,
    /// MD5 of the icon, when it has one. The icon itself comes as the
    /// packet's payload only when it changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_hash: Option<String>,
}

/// Ask the peer for every notification it shows now.
pub fn build_request_all(id: impl Into<Number>) -> Result<Packet, BodyError> {
    Packet::from_body(
        id,
        REQUEST_PACKET_TYPE,
        &serde_json::json!({ "request": true }),
    )
}

/// Dismiss notification `notification_id` on the peer.
pub fn build_cancel(id: impl Into<Number>, notification_id: &str) -> Result<Packet, BodyError> {
    Packet::from_body(
        id,
        REQUEST_PACKET_TYPE,
        &serde_json::json!({ "cancel": notification_id }),
    )
}

/// Answer the notification whose `requestReplyId` is `reply_id`.
pub fn build_reply(
    id: impl Into<Number>,
    reply_id: &str,
    message: &str,
) -> Result<Packet, BodyError> {
    Packet::from_body(
        id,
        REPLY_PACKET_TYPE,
        &serde_json::json!({ "requestReplyId": reply_id, "message": message }),
    )
}

/// Press the button labelled `action` on notification `notification_id`.
pub fn build_action(
    id: impl Into<Number>,
    notification_id: &str,
    action: &str,
) -> Result<Packet, BodyError> {
    Packet::from_body(
        id,
        ACTION_PACKET_TYPE,
        &serde_json::json!({ "key": notification_id, "action": action }),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_what_android_sends() {
        let packet = Packet::from_body(
            1_u64,
            PACKET_TYPE,
            &json!({
                "id": "0|com.google.android.apps.messaging|1|null|10123",
                "isClearable": true,
                "appName": "Messages",
                "time": "1727300000000",
                "silent": false,
                "requestReplyId": "5b1e",
                "ticker": "Ana: Dinner?",
                "title": "Ana",
                "text": "Dinner?",
                "actions": ["Mark as read"],
                "payloadHash": "9e107d9d372bb6826bd81d3542a419d6",
            }),
        )
        .unwrap();
        let body: NotificationBody = packet.body_as().unwrap();
        assert_eq!(body.app_name.as_deref(), Some("Messages"));
        assert_eq!(body.request_reply_id.as_deref(), Some("5b1e"));
        assert_eq!(body.actions, Some(vec!["Mark as read".into()]));
        assert!(!body.is_cancel);

        let removed = Packet::from_body(2_u64, PACKET_TYPE, &json!({"id": "a", "isCancel": true}))
            .unwrap()
            .body_as::<NotificationBody>()
            .unwrap();
        assert!(removed.is_cancel);
        // Android sends `"actions": null` for a notification without buttons.
        let plain = Packet::from_body(3_u64, PACKET_TYPE, &json!({"id": "b", "actions": null}))
            .unwrap()
            .body_as::<NotificationBody>()
            .unwrap();
        assert_eq!(plain.actions, None);
    }

    #[test]
    fn builds_what_android_reads() {
        assert_eq!(
            build_request_all(1_u64).unwrap().body,
            json!({"request": true}).as_object().unwrap().clone()
        );
        assert_eq!(
            build_cancel(1_u64, "a").unwrap().body,
            json!({"cancel": "a"}).as_object().unwrap().clone()
        );
        let reply = build_reply(1_u64, "5b1e", "Yes!").unwrap();
        assert_eq!(reply.packet_type, REPLY_PACKET_TYPE);
        assert_eq!(
            reply.body,
            json!({"requestReplyId": "5b1e", "message": "Yes!"})
                .as_object()
                .unwrap()
                .clone()
        );
        let action = build_action(1_u64, "a", "Mark as read").unwrap();
        assert_eq!(action.packet_type, ACTION_PACKET_TYPE);
        assert_eq!(
            action.body,
            json!({"key": "a", "action": "Mark as read"})
                .as_object()
                .unwrap()
                .clone()
        );
    }
}
