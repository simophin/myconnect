use sha2::{Digest, Sha256};

/// Calculate the protocol-v8 pairing verification code.
///
/// The arguments are complete DER-encoded SubjectPublicKeyInfo values. They
/// are ordered bytewise so both peers calculate the same value regardless of
/// which initiated pairing.
pub fn verification_code(
    local_public_key_der: &[u8],
    peer_public_key_der: &[u8],
    pair_request_timestamp: i64,
) -> String {
    let (maximum, minimum) = if local_public_key_der >= peer_public_key_der {
        (local_public_key_der, peer_public_key_der)
    } else {
        (peer_public_key_der, local_public_key_der)
    };

    let mut digest = Sha256::new();
    digest.update(maximum);
    digest.update(minimum);
    digest.update(pair_request_timestamp.to_string().as_bytes());
    let digest = digest.finalize();

    digest[..4]
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_vector_proves_order_timestamp_and_format() {
        assert_eq!(
            verification_code(b"alpha", b"beta", 1_758_510_000),
            "010D5010"
        );
        assert_eq!(
            verification_code(b"beta", b"alpha", 1_758_510_000),
            "010D5010"
        );

        let code = verification_code(&[0, 255], &[255, 0], 0);
        assert_eq!(code, "514D3D2B");
        assert_eq!(code.len(), 8);
        assert!(
            code.bytes()
                .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte))
        );
        assert_ne!(
            verification_code(b"alpha", b"beta", 1_758_510_001),
            "010D5010"
        );
    }
}
