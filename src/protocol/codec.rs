use crate::protocol::Packet;
use thiserror::Error;

/// Incrementally frames newline-delimited KDE Connect packets.
///
/// The internal allocation never grows beyond `max_line_len`, even when a peer
/// sends an unterminated or very large line.
#[derive(Debug)]
pub struct PacketCodec {
    buffer: Vec<u8>,
    max_line_len: usize,
}

impl PacketCodec {
    pub fn new(max_line_len: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(max_line_len.min(8 * 1024)),
            max_line_len,
        }
    }

    /// Consume a fragment and return every complete packet it contains.
    pub fn decode(&mut self, input: &[u8]) -> Result<Vec<Packet>, CodecError> {
        let mut packets = Vec::new();

        for &byte in input {
            if byte == b'\n' {
                let line = self.buffer.strip_suffix(b"\r").unwrap_or(&self.buffer);
                let packet = serde_json::from_slice(line).map_err(CodecError::InvalidJson);
                self.buffer.clear();
                packets.push(packet?);
            } else if self.buffer.len() == self.max_line_len {
                self.buffer.clear();
                return Err(CodecError::LineTooLong {
                    max: self.max_line_len,
                });
            } else {
                self.buffer.push(byte);
            }
        }

        Ok(packets)
    }

    /// Serialize one packet with its newline delimiter.
    pub fn encode(&self, packet: &Packet) -> Result<Vec<u8>, CodecError> {
        let mut encoded = serde_json::to_vec(packet).map_err(CodecError::InvalidJson)?;
        if encoded.len() > self.max_line_len {
            return Err(CodecError::LineTooLong {
                max: self.max_line_len,
            });
        }
        encoded.push(b'\n');
        Ok(encoded)
    }

    /// Bytes retained for an incomplete line. Exposed for observability/tests.
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }
}

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("packet line exceeds the configured {max}-byte limit")]
    LineTooLong { max: usize },
    #[error("invalid packet JSON")]
    InvalidJson(#[source] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTITY: &[u8] = include_bytes!("../../tests/fixtures/identity.json");
    const PAIRING_REQUEST: &[u8] = include_bytes!("../../tests/fixtures/pairing_request.json");
    const PAYLOAD: &[u8] = include_bytes!("../../tests/fixtures/payload.json");

    #[test]
    fn decodes_a_fragmented_packet() {
        let mut codec = PacketCodec::new(4096);
        let split = IDENTITY.len() / 2;

        assert!(codec.decode(&IDENTITY[..split]).unwrap().is_empty());
        let packets = codec.decode(&IDENTITY[split..]).unwrap();

        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].packet_type, "kdeconnect.identity");
    }

    #[test]
    fn decodes_multiple_packets_from_one_buffer() {
        let mut input = Vec::from(PAIRING_REQUEST);
        input.extend_from_slice(PAYLOAD);

        let packets = PacketCodec::new(4096).decode(&input).unwrap();

        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0].packet_type, "kdeconnect.pair");
        assert_eq!(packets[1].packet_type, "kdeconnect.share.request");
    }

    #[test]
    fn rejects_oversized_lines_without_retaining_them() {
        let mut codec = PacketCodec::new(8);

        let error = codec.decode(b"123456789").unwrap_err();

        assert!(matches!(error, CodecError::LineTooLong { max: 8 }));
        assert_eq!(codec.buffered_len(), 0);
    }
}
