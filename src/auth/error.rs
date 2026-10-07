//! What can go wrong signing in to Google.

use std::fmt;

/// Why grpy couldn't sign in to Google or get an access token.
///
/// No variant holds a token or client secret, so errors are safe to print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// No refresh token is stored: the user never signed in.
    NotSignedIn,
    /// Google rejected the stored refresh token because it expired or the
    /// user revoked grpy's access.
    Revoked,
    /// The user declined access on Google's consent page.
    Denied,
    /// The user signed in but didn't allow access to calendar events.
    MissingScope,
    /// Google answered the code exchange without a refresh token.
    NoRefreshToken,
    /// The browser redirect carried a `state` that doesn't match this
    /// sign-in, so it may not come from the request grpy started.
    StateMismatch,
    /// The browser redirect couldn't be understood.
    BadRedirect(String),
    /// Google's OAuth server returned another error, such as
    /// `invalid_client` for a wrong client ID or secret.
    OAuth {
        /// The OAuth error code.
        error: String,
        /// Google's explanation, when it gave one.
        description: Option<String>,
    },
    /// Google couldn't be reached or sent something unexpected.
    Network(String),
    /// The refresh token couldn't be read or saved.
    Store(String),
    /// A local failure: the redirect listener or the random number
    /// generator.
    Io(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSignedIn => f.write_str("not signed in to Google; run `grpy auth google`"),
            Self::Revoked => f.write_str(
                "Google rejected grpy's saved sign-in (it expired or access was revoked); \
                 re-run `grpy auth google`",
            ),
            Self::Denied => f.write_str(
                "Google sign-in was cancelled; run `grpy auth google` again and allow access",
            ),
            Self::MissingScope => f.write_str(
                "calendar access wasn't granted; re-run `grpy auth google` and allow grpy to \
                 view and edit events on your calendars",
            ),
            Self::NoRefreshToken => f.write_str(
                "Google didn't return a refresh token; remove grpy at \
                 https://myaccount.google.com/permissions and re-run `grpy auth google`",
            ),
            Self::StateMismatch => f.write_str(
                "the Google sign-in response didn't match this request; \
                 re-run `grpy auth google`",
            ),
            Self::BadRedirect(why) => write!(f, "unexpected Google sign-in redirect: {why}"),
            Self::OAuth {
                error,
                description: Some(description),
            } => write!(f, "Google OAuth error `{error}`: {description}"),
            Self::OAuth {
                error,
                description: None,
            } => write!(f, "Google OAuth error `{error}`"),
            Self::Network(why) => write!(f, "couldn't talk to Google: {why}"),
            Self::Store(why) => write!(f, "couldn't access the saved Google sign-in: {why}"),
            Self::Io(why) => write!(f, "Google sign-in failed: {why}"),
        }
    }
}

impl std::error::Error for AuthError {}
