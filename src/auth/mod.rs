//! Signing in to Google Calendar.
//!
//! grpy uses the OAuth 2.0 flow for installed apps: it opens Google's
//! consent page in the browser, receives the authorization code on a
//! loopback redirect (`http://127.0.0.1:<port>`), and trades it for a
//! refresh token, protected by PKCE. Only the
//! [`CALENDAR_EVENTS_SCOPE`] is requested.

mod error;
pub mod google;
mod pkce;

pub use error::AuthError;
pub use pkce::Pkce;

/// The only scope grpy asks for: read and write events, without access to
/// calendar settings or sharing.
pub const CALENDAR_EVENTS_SCOPE: &str = "https://www.googleapis.com/auth/calendar.events";
