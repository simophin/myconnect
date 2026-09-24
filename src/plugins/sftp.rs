//! `kdeconnect.sftp.request` and `kdeconnect.sftp` packet models.
//!
//! KDE Connect lets a desktop browse a phone's storage over SFTP. The desktop
//! sends `kdeconnect.sftp.request` with `startBrowsing: true`. The phone
//! starts (or keeps) an SFTP server and answers with `kdeconnect.sftp`,
//! carrying the port, a user name, a fresh password, and the storage roots
//! it exposes. The file access itself then happens over SSH on that port;
//! nothing else about it travels on the control channel.
//!
//! Only KDE Connect for Android serves files. It sends the same packet type
//! with just `errorMessage` when it can't (no storage permission, nothing
//! configured), and with just `serverRunning: false` when its plugin reloads,
//! which invalidates any session opened against the old server.
//!
//! Never log the password.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

use crate::protocol::{BodyError, Packet};

/// Packet type and capability identifier for asking a peer to serve files.
pub const REQUEST_PACKET_TYPE: &str = "kdeconnect.sftp.request";
/// Packet type and capability identifier for a peer's SFTP server details.
pub const PACKET_TYPE: &str = "kdeconnect.sftp";

/// Body of a `kdeconnect.sftp.request` packet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpRequestBody {
    #[serde(default)]
    pub start_browsing: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Body of a `kdeconnect.sftp` packet. Every field is optional because the
/// same packet type carries an offer, an error, or a server-stopped notice.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpBody {
    /// The address the server listens on. Clients connect to the address the
    /// control connection uses instead, as KDE Connect's own clients do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// The only root when there is exactly one, else `/`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Absolute paths of the storage roots, paired with `path_names`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multi_paths: Option<Vec<String>>,
    /// Display names of the storage roots in `multi_paths`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_running: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl fmt::Debug for SftpBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SftpBody")
            .field("ip", &self.ip)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("path", &self.path)
            .field("multi_paths", &self.multi_paths)
            .field("path_names", &self.path_names)
            .field("error_message", &self.error_message)
            .field("server_running", &self.server_running)
            .finish_non_exhaustive()
    }
}

/// One storage root a peer exposes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SftpRoot {
    pub path: String,
    pub name: String,
}

/// What a `kdeconnect.sftp` packet means.
#[derive(Clone, PartialEq, Eq)]
pub enum SftpReply {
    /// The server is up; connect with these details.
    Offer {
        port: u16,
        user: String,
        password: String,
        roots: Vec<SftpRoot>,
    },
    /// The peer can't serve files, for the reason it gives.
    Error(String),
    /// The peer's server stopped; sessions against it are gone.
    Stopped,
}

impl fmt::Debug for SftpReply {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offer {
                port, user, roots, ..
            } => formatter
                .debug_struct("Offer")
                .field("port", port)
                .field("user", user)
                .field("roots", roots)
                .finish_non_exhaustive(),
            Self::Error(message) => formatter.debug_tuple("Error").field(message).finish(),
            Self::Stopped => formatter.write_str("Stopped"),
        }
    }
}

impl SftpBody {
    /// Interpret the body, or `None` if it is none of an offer, an error, or
    /// a stop notice.
    pub fn reply(&self) -> Option<SftpReply> {
        if let Some(message) = &self.error_message {
            return Some(SftpReply::Error(message.clone()));
        }
        if let (Some(port), Some(user), Some(password)) = (self.port, &self.user, &self.password) {
            return Some(SftpReply::Offer {
                port,
                user: user.clone(),
                password: password.clone(),
                roots: self.roots(),
            });
        }
        if self.server_running == Some(false) {
            return Some(SftpReply::Stopped);
        }
        None
    }

    /// The storage roots: `multiPaths` named by `pathNames` where both are
    /// present and line up, else the single `path`. Roots that aren't
    /// absolute paths are dropped.
    fn roots(&self) -> Vec<SftpRoot> {
        let named: Vec<SftpRoot> = match (&self.multi_paths, &self.path_names) {
            (Some(paths), Some(names)) if paths.len() == names.len() => paths
                .iter()
                .zip(names)
                .map(|(path, name)| SftpRoot {
                    path: path.clone(),
                    name: name.clone(),
                })
                .collect(),
            (Some(paths), _) => paths
                .iter()
                .map(|path| SftpRoot {
                    path: path.clone(),
                    name: default_root_name(path),
                })
                .collect(),
            (None, _) => self
                .path
                .iter()
                .map(|path| SftpRoot {
                    path: path.clone(),
                    name: default_root_name(path),
                })
                .collect(),
        };
        named
            .into_iter()
            .filter(|root| root.path.starts_with('/') && !root.path.contains('\0'))
            .collect()
    }
}

fn default_root_name(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("/")
        .to_owned()
}

/// Build a `kdeconnect.sftp.request` asking the peer to start serving.
pub fn build_request_packet(id: impl Into<Number>) -> Result<Packet, BodyError> {
    Packet::from_body(
        id,
        REQUEST_PACKET_TYPE,
        &SftpRequestBody {
            start_browsing: true,
            extra: Map::new(),
        },
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn body(value: Value) -> SftpBody {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn request_asks_to_start_browsing() {
        let packet = build_request_packet(1_u64).unwrap();
        assert_eq!(packet.packet_type, REQUEST_PACKET_TYPE);
        assert_eq!(packet.body["startBrowsing"], json!(true));
    }

    #[test]
    fn an_android_offer_lists_its_named_roots() {
        let reply = body(json!({
            "ip": "192.168.1.20",
            "port": 1739,
            "user": "kdeconnect",
            "password": "secret",
            "path": "/",
            "multiPaths": ["/storage/emulated/0", "/storage/1234-ABCD"],
            "pathNames": ["All files", "SD card"],
        }))
        .reply();
        assert_eq!(
            reply,
            Some(SftpReply::Offer {
                port: 1739,
                user: "kdeconnect".into(),
                password: "secret".into(),
                roots: vec![
                    SftpRoot {
                        path: "/storage/emulated/0".into(),
                        name: "All files".into(),
                    },
                    SftpRoot {
                        path: "/storage/1234-ABCD".into(),
                        name: "SD card".into(),
                    },
                ],
            })
        );
    }

    #[test]
    fn an_offer_without_names_falls_back_to_path_names() {
        let Some(SftpReply::Offer { roots, .. }) = body(json!({
            "port": 1739,
            "user": "kdeconnect",
            "password": "secret",
            "path": "/storage/emulated/0/",
        }))
        .reply() else {
            panic!("expected an offer");
        };
        assert_eq!(
            roots,
            [SftpRoot {
                path: "/storage/emulated/0/".into(),
                name: "0".into(),
            }]
        );
    }

    #[test]
    fn relative_roots_are_dropped() {
        let Some(SftpReply::Offer { roots, .. }) = body(json!({
            "port": 1739,
            "user": "kdeconnect",
            "password": "secret",
            "multiPaths": ["relative", "/ok"],
            "pathNames": ["Bad", "Good"],
        }))
        .reply() else {
            panic!("expected an offer");
        };
        assert_eq!(
            roots,
            [SftpRoot {
                path: "/ok".into(),
                name: "Good".into(),
            }]
        );
    }

    #[test]
    fn errors_and_stop_notices_are_recognized() {
        assert_eq!(
            body(json!({"errorMessage": "No permission"})).reply(),
            Some(SftpReply::Error("No permission".into()))
        );
        assert_eq!(
            body(json!({"serverRunning": false})).reply(),
            Some(SftpReply::Stopped)
        );
        assert_eq!(body(json!({})).reply(), None);
    }

    #[test]
    fn debug_output_never_contains_the_password() {
        let body = body(json!({
            "port": 1739,
            "user": "kdeconnect",
            "password": "hunter2",
        }));
        assert!(!format!("{body:?}").contains("hunter2"));
        assert!(!format!("{:?}", body.reply().unwrap()).contains("hunter2"));
    }
}
