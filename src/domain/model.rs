//! Venues, events and their locations.

use chrono::{DateTime, FixedOffset};
use url::Url;

use super::{EventId, ProviderId, VenueId};

/// A point on the map with a human-readable label.
#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    /// Latitude in decimal degrees (WGS 84).
    pub lat: f64,
    /// Longitude in decimal degrees (WGS 84).
    pub lon: f64,
    /// Display name for the location, such as `"Fort Lauderdale, FL"`.
    pub label: String,
}

/// A place that hosts shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Venue {
    /// Provider-namespaced ID; also identifies which provider reported it.
    pub id: VenueId,
    /// Venue name as shown to the user.
    pub name: String,
    /// Street address, when the provider has one. Map-based discovery
    /// often has coordinates but no address.
    pub address: Option<String>,
    /// Where the venue is.
    pub location: Location,
}

impl Venue {
    /// The provider that reported this venue, taken from its [`VenueId`].
    pub fn provider(&self) -> &ProviderId {
        self.id.provider()
    }
}

/// A single show at a venue.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Provider-namespaced ID; also identifies which provider reported it.
    pub id: EventId,
    /// The venue the show is at.
    pub venue_id: VenueId,
    /// Show title as listed by the provider.
    pub title: String,
    /// Performers, headliner first. May be empty when the provider only
    /// gives a title.
    pub artists: Vec<String>,
    /// When the show starts, in the venue's local time with its UTC
    /// offset, so calendar entries land at the right instant.
    pub starts_at: DateTime<FixedOffset>,
    /// When doors open, if the provider says.
    pub doors_at: Option<DateTime<FixedOffset>>,
    /// Where to buy tickets, if known.
    pub ticket_url: Option<Url>,
    /// The page this event was read from. Always present, so a calendar
    /// entry can always link back to the show.
    pub source_url: Url,
}

impl Event {
    /// The provider that reported this event, taken from its [`EventId`].
    pub fn provider(&self) -> &ProviderId {
        self.id.provider()
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn revolution_live() -> Venue {
        Venue {
            id: "ics:revolution-live".parse().unwrap(),
            name: "Revolution Live".into(),
            address: Some("100 SW 3rd Ave, Fort Lauderdale, FL 33312".into()),
            location: Location {
                lat: 26.1194,
                lon: -80.1462,
                label: "Fort Lauderdale, FL".into(),
            },
        }
    }

    fn show(starts_at: &str) -> Event {
        Event {
            id: "ics:https://www.jointherevolution.net/events/some-band/"
                .parse()
                .unwrap(),
            venue_id: revolution_live().id,
            title: "Some Band".into(),
            artists: vec!["Some Band".into(), "Opener".into()],
            starts_at: DateTime::parse_from_rfc3339(starts_at).unwrap(),
            doors_at: None,
            ticket_url: None,
            source_url: "https://www.jointherevolution.net/events/some-band/"
                .parse()
                .unwrap(),
        }
    }

    #[test]
    fn venue_provider_comes_from_its_id() {
        assert_eq!(revolution_live().provider().to_string(), "ics");
    }

    #[test]
    fn event_provider_comes_from_its_id() {
        assert_eq!(
            show("2026-10-17T20:00:00-04:00").provider().to_string(),
            "ics"
        );
    }

    #[test]
    fn starts_at_keeps_the_venue_local_offset() {
        // 8pm Eastern Daylight Time is midnight UTC the next day.
        let event = show("2026-10-17T20:00:00-04:00");

        assert_eq!(event.starts_at.offset().local_minus_utc(), -4 * 3600);
        assert_eq!(event.starts_at.format("%H:%M").to_string(), "20:00");
        assert_eq!(
            event.starts_at.with_timezone(&Utc),
            Utc.with_ymd_and_hms(2026, 10, 18, 0, 0, 0).unwrap()
        );
    }

    #[test]
    fn event_without_ticket_url_still_links_to_its_source() {
        let event = show("2026-10-17T20:00:00-04:00");

        assert!(event.ticket_url.is_none());
        assert_eq!(
            event.source_url.as_str(),
            "https://www.jointherevolution.net/events/some-band/"
        );
    }
}
