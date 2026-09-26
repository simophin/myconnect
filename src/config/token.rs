use std::fmt;

use thiserror::Error;
use uuid::Uuid;

/// Upper bound on a caller-supplied token, so a misconfigured value cannot
/// turn every request's header comparison into an unbounded operation.
const MAX_TOKEN_LENGTH: usize = 256;

/// Optional shared secret used to authenticate local API clients.
///
/// The control API only requires authentication when the daemon was started
/// with one. `ferry-cli run` takes one from its flags, for that run only; the
/// app always has one, kept in `api.json` ([`super::ApiFile`]) so the CLI
/// keeps working across its restarts.
///
/// `Debug` is implemented but redacts the value; the type is intentionally not
/// serializable.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiToken(String);

impl ApiToken {
    /// Accept a caller-chosen secret. It must be non-empty, at most 256 bytes,
    /// and consist only of visible ASCII so it round-trips through an HTTP
    /// `Authorization` header unchanged.
    pub fn from_secret(value: impl Into<String>) -> Result<Self, ApiTokenError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_TOKEN_LENGTH
            || !value.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(ApiTokenError::Invalid);
        }
        Ok(Self(value))
    }

    /// Generate a random 64-character hex token.
    pub fn generate() -> Self {
        Self(format!(
            "{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        ))
    }

    /// Reveal the token only at the boundary that constructs authentication.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    pub(crate) fn constant_time_matches(&self, candidate: &str) -> bool {
        if candidate.len() != self.0.len() {
            return false;
        }
        self.0
            .bytes()
            .zip(candidate.bytes())
            .fold(0_u8, |difference, (expected, actual)| {
                difference | (expected ^ actual)
            })
            == 0
    }
}

impl fmt::Debug for ApiToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ApiToken(<redacted>)")
    }
}

#[derive(Debug, Error)]
pub enum ApiTokenError {
    #[error("API token must be 1-256 visible ASCII characters")]
    Invalid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_random_and_well_formed() {
        let first = ApiToken::generate();
        let second = ApiToken::generate();
        assert_ne!(first, second);
        assert_eq!(first.expose_secret().len(), 64);
        assert!(
            first
                .expose_secret()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
        assert!(first.constant_time_matches(first.expose_secret()));
        assert!(!first.constant_time_matches(second.expose_secret()));
        assert!(!first.constant_time_matches("wrong"));
    }

    #[test]
    fn caller_secrets_are_validated() {
        assert!(ApiToken::from_secret("s3cr3t-token").is_ok());
        assert!(ApiToken::from_secret("").is_err());
        assert!(ApiToken::from_secret("has space").is_err());
        assert!(ApiToken::from_secret("line\nbreak").is_err());
        assert!(ApiToken::from_secret("é").is_err());
        assert!(ApiToken::from_secret("a".repeat(MAX_TOKEN_LENGTH + 1)).is_err());
    }

    #[test]
    fn debug_output_never_reveals_the_secret() {
        let token = ApiToken::from_secret("visible-secret").unwrap();
        assert!(!format!("{token:?}").contains("visible-secret"));
    }
}
