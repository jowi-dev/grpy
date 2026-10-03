//! A string that must never end up in logs.

use std::fmt;

/// An API key, token or other credential.
///
/// `Debug` prints `Secret(<redacted>)` and there is no `Display`, so a
/// secret can't leak through `{:?}`, `dbg!` or an error message by
/// accident. Call [`Secret::expose`] at the one place that actually sends
/// the value, such as an HTTP request.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wraps a credential.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The raw credential. Only pass this to the service it belongs to;
    /// never print or log it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_is_redacted() {
        let secret = Secret::new("tm-abc123");

        let shown = format!("{secret:?} {:#?}", Some(&secret));

        assert!(!shown.contains("tm-abc123"), "leaked: {shown}");
        assert!(shown.contains("redacted"));
    }

    #[test]
    fn expose_returns_the_value() {
        assert_eq!(Secret::new("tm-abc123").expose(), "tm-abc123");
    }
}
