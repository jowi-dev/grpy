//! Proof Key for Code Exchange ([RFC 7636]).
//!
//! [RFC 7636]: https://www.rfc-editor.org/rfc/rfc7636

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

use crate::config::Secret;

/// A PKCE code verifier and its `S256` challenge.
///
/// The challenge goes in the authorization URL; the verifier is sent only
/// with the code exchange, proving the same app started the flow.
#[derive(Debug, Clone)]
pub struct Pkce {
    /// Sent with the code exchange. Kept secret until then.
    pub verifier: Secret,
    /// `BASE64URL(SHA256(verifier))`, sent in the authorization URL.
    pub challenge: String,
}

impl Pkce {
    /// A fresh verifier from 32 random bytes (43 URL-safe characters).
    ///
    /// # Errors
    ///
    /// When the operating system's random number generator fails.
    pub fn generate() -> Result<Self, getrandom::Error> {
        Ok(Self::from_verifier(random_token()?))
    }

    /// The pair for a known verifier.
    pub fn from_verifier(verifier: impl Into<String>) -> Self {
        let verifier = verifier.into();
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        Self {
            verifier: Secret::new(verifier),
            challenge,
        }
    }
}

/// 32 random bytes as 43 URL-safe characters, for PKCE verifiers and the
/// OAuth `state` parameter.
pub(super) fn random_token() -> Result<String, getrandom::Error> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_is_base64url_sha256_of_the_verifier() {
        let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mJ92K9mfGx1gq74d6XYvUKOh2HlAQs");

        assert_eq!(
            pkce.challenge,
            "06aTZDjdj5HRPy5S-SZuIiTIBQqvQXWhUi0w-idGegA"
        );
        assert_eq!(
            pkce.verifier.expose(),
            "dBjftJeZ4CVP-mJ92K9mfGx1gq74d6XYvUKOh2HlAQs"
        );
    }

    #[test]
    fn generated_verifiers_are_43_url_safe_chars_and_unique() {
        let first = Pkce::generate().unwrap();
        let second = Pkce::generate().unwrap();

        let verifier = first.verifier.expose();
        assert_eq!(verifier.len(), 43);
        assert!(
            verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "{verifier}"
        );
        assert_ne!(first.verifier, second.verifier);
    }
}
