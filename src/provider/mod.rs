//! Sources of venues and events.
//!
//! Each data source (OpenStreetMap, venue scrapers, Ticketmaster, ...)
//! implements [`EventProvider`], mapping its own data into [`crate::domain`]
//! types. The TUI only talks to this trait, so new sources can be added
//! without touching it. [`FakeProvider`] serves canned data for tests and
//! TUI development.

mod fake;
pub mod ticketmaster;

use std::fmt;
use std::future::Future;

use crate::domain::{DateRange, Event, Location, Venue, VenueId};

pub use fake::FakeProvider;
pub use ticketmaster::Ticketmaster;

/// Error returned by an [`EventProvider`].
#[derive(Debug)]
pub enum Error {
    /// The provider has no venue with this ID.
    UnknownVenue(VenueId),
    /// Anything else the provider hit: network, HTTP status, parsing.
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownVenue(id) => write!(f, "unknown venue `{id}`"),
            Self::Other(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UnknownVenue(_) => None,
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

/// Result type returned by [`EventProvider`] methods.
pub type Result<T> = std::result::Result<T, Error>;

/// A source of venues and their upcoming shows.
///
/// Implementors can write these as `async fn`; the returned futures must be
/// `Send` so callers can run them on the multi-threaded tokio runtime.
pub trait EventProvider {
    /// Venues within `radius_km` kilometres of `loc`, nearest first.
    fn venues_near(
        &self,
        loc: &Location,
        radius_km: u32,
    ) -> impl Future<Output = Result<Vec<Venue>>> + Send;

    /// Shows at `venue` starting inside `window`, earliest first.
    ///
    /// Returns [`Error::UnknownVenue`] if this provider does not know
    /// `venue`, and an empty list if it knows the venue but has no shows in
    /// the window.
    fn upcoming_events(
        &self,
        venue: &VenueId,
        window: DateRange,
    ) -> impl Future<Output = Result<Vec<Event>>> + Send;
}
