//! Signing in to Google Calendar.
//!
//! grpy uses the OAuth 2.0 flow for installed apps: it opens Google's
//! consent page in the browser, receives the authorization code on a
//! loopback redirect (`http://127.0.0.1:<port>`), and trades it for a
//! refresh token, protected by PKCE. Only the
//! [`CALENDAR_EVENTS_SCOPE`] is requested.

mod error;
pub mod google;
mod loopback;
mod pkce;
mod session;
mod store;

pub use error::AuthError;
pub use loopback::Loopback;
pub use pkce::Pkce;
pub use session::{EXPIRY_MARGIN, GoogleAuth};
pub use store::{
    FallbackStore, FileStore, KEYRING_SERVICE, KEYRING_USER, KeyringStore, TOKEN_FILE_NAME,
    TokenStore,
};

/// The only scope grpy asks for: read and write events, without access to
/// calendar settings or sharing.
pub const CALENDAR_EVENTS_SCOPE: &str = "https://www.googleapis.com/auth/calendar.events";
