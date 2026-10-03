//! Provider-agnostic domain types.
//!
//! Providers (venue scrapers, Ticketmaster, ...) map their data into these
//! types. The TUI and calendar code only ever see this module, never
//! provider-specific JSON or HTML.

mod id;
mod model;

pub use id::{EventId, ParseIdError, ProviderId, VenueId};
pub use model::{Event, Location, Venue};
