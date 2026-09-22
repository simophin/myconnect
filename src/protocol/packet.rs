use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Number, Value};
use thiserror::Error;

/// A KDE Connect network packet.
///
/// Packet types and body fields intentionally remain open-ended so newer peers
/// can be decoded without discarding data that this version does not know.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Packet {
    pub id: Number,
    #[serde(rename = "type")]
    pub packet_type: String,
    pub body: Map<String, Value>,
    #[serde(rename = "payloadSize", skip_serializing_if = "Option::is_none")]
    pub payload_size: Option<i64>,
    #[serde(
        rename = "payloadTransferInfo",
        skip_serializing_if = "Option::is_none"
    )]
    pub payload_transfer_info: Option<Map<String, Value>>,
    /// Additional envelope fields sent by newer implementations.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Packet {
    /// Deserialize the packet body as a known packet type.
    pub fn body_as<T: DeserializeOwned>(&self) -> Result<T, BodyError> {
        serde_json::from_value(Value::Object(self.body.clone())).map_err(BodyError::Deserialize)
    }

    /// Construct a packet from a serializable typed body.
    pub fn from_body<T: Serialize>(
        id: impl Into<Number>,
        packet_type: impl Into<String>,
        body: &T,
    ) -> Result<Self, BodyError> {
        let body = serde_json::to_value(body).map_err(BodyError::Serialize)?;
        let Value::Object(body) = body else {
            return Err(BodyError::NotAnObject);
        };

        Ok(Self {
            id: id.into(),
            packet_type: packet_type.into(),
            body,
            payload_size: None,
            payload_transfer_info: None,
            extra: Map::new(),
        })
    }
}

#[derive(Debug, Error)]
pub enum BodyError {
    #[error("packet body could not be decoded")]
    Deserialize(#[source] serde_json::Error),
    #[error("packet body could not be encoded")]
    Serialize(#[source] serde_json::Error),
    #[error("packet body must serialize as a JSON object")]
    NotAnObject,
}

/// The icon category advertised by an identity packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Desktop,
    Laptop,
    Phone,
    Tablet,
    Tv,
}

/// Body of a `kdeconnect.identity` packet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityBody {
    pub device_id: String,
    pub device_name: String,
    pub device_type: DeviceType,
    pub incoming_capabilities: Vec<String>,
    pub outgoing_capabilities: Vec<String>,
    pub protocol_version: u8,
    /// Transport-specific and future identity properties, such as `tcpPort`.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl IdentityBody {
    /// Validate the constraints in the KDE Connect identity schema.
    pub fn validate(&self) -> Result<(), IdentityValidationError> {
        let id_len = self.device_id.chars().count();
        if !(32..=38).contains(&id_len)
            || !self
                .device_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        {
            return Err(IdentityValidationError::DeviceId);
        }

        let name_len = self.device_name.chars().count();
        if !(1..=32).contains(&name_len)
            || self.device_name.chars().any(is_forbidden_name_character)
        {
            return Err(IdentityValidationError::DeviceName);
        }

        if !matches!(self.protocol_version, 7 | 8) {
            return Err(IdentityValidationError::ProtocolVersion(
                self.protocol_version,
            ));
        }

        Ok(())
    }
}

fn is_forbidden_name_character(character: char) -> bool {
    matches!(
        character,
        '"' | '\'' | ',' | ';' | ':' | '.' | '!' | '?' | '(' | ')' | '[' | ']' | '<' | '>'
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum IdentityValidationError {
    #[error("device ID must be 32 to 38 ASCII letters, digits, hyphens, or underscores")]
    DeviceId,
    #[error("device name must be 1 to 32 characters and contain no reserved punctuation")]
    DeviceName,
    #[error("unsupported KDE Connect protocol version {0}; expected 7 or 8")]
    ProtocolVersion(u8),
}

/// Body of a `kdeconnect.pair` packet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingBody {
    pub pair: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_identity() -> IdentityBody {
        IdentityBody {
            device_id: "740bd4b9b4184ee497d6caf1da8151be".into(),
            device_name: "FOSS Phone".into(),
            device_type: DeviceType::Phone,
            incoming_capabilities: vec!["kdeconnect.mock.echo".into()],
            outgoing_capabilities: vec!["kdeconnect.mock.echo".into()],
            protocol_version: 8,
            extra: Map::new(),
        }
    }

    #[test]
    fn validates_identity_schema_constraints() {
        assert_eq!(valid_identity().validate(), Ok(()));

        let mut identity = valid_identity();
        identity.device_id = "too-short".into();
        assert_eq!(identity.validate(), Err(IdentityValidationError::DeviceId));

        let mut identity = valid_identity();
        identity.device_name = "bad.name".into();
        assert_eq!(
            identity.validate(),
            Err(IdentityValidationError::DeviceName)
        );

        let mut identity = valid_identity();
        identity.protocol_version = 9;
        assert_eq!(
            identity.validate(),
            Err(IdentityValidationError::ProtocolVersion(9))
        );
    }

    #[test]
    fn typed_body_preserves_unknown_fields() {
        let mut packet = Packet::from_body(0, "kdeconnect.identity", &valid_identity()).unwrap();
        packet.body.insert("tcpPort".into(), json!(1716));

        let identity: IdentityBody = packet.body_as().unwrap();
        assert_eq!(identity.extra["tcpPort"], 1716);

        let rebuilt = Packet::from_body(0, "kdeconnect.identity", &identity).unwrap();
        assert_eq!(rebuilt.body["tcpPort"], 1716);
    }
}
