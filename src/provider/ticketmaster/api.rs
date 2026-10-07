//! Discovery API response shapes and their mapping to domain types.

use serde::Deserialize;

use crate::domain::{Location, Venue, VenueId};

/// One page of `/discovery/v2/venues` results.
#[derive(Debug, Default, PartialEq)]
pub(super) struct VenuePage {
    /// Venues on this page that have coordinates; the rest are dropped.
    pub venues: Vec<Venue>,
    /// Total number of pages the search has.
    pub total_pages: u32,
}

/// Parses a `/discovery/v2/venues` response body.
pub(super) fn parse_venue_page(body: &str) -> Result<VenuePage, String> {
    let raw: RawVenuePage = serde_json::from_str(body)
        .map_err(|err| format!("unexpected Ticketmaster venues response: {err}"))?;
    Ok(VenuePage {
        venues: raw
            .embedded
            .venues
            .into_iter()
            .filter_map(RawVenue::into_venue)
            .collect(),
        total_pages: raw.page.total_pages,
    })
}

/// Parses a `/discovery/v2/events/{id}` response body into the event's
/// venue.
pub(super) fn parse_event_venue(body: &str) -> Result<Venue, String> {
    let raw: RawEvent = serde_json::from_str(body)
        .map_err(|err| format!("unexpected Ticketmaster event response: {err}"))?;
    raw.embedded
        .venues
        .into_iter()
        .next()
        .and_then(RawVenue::into_venue)
        .ok_or_else(|| "Ticketmaster event has no venue with coordinates".into())
}

#[derive(Deserialize)]
struct RawEvent {
    #[serde(rename = "_embedded", default)]
    embedded: RawEmbedded,
}

#[derive(Deserialize)]
struct RawVenuePage {
    #[serde(rename = "_embedded", default)]
    embedded: RawEmbedded,
    page: RawPage,
}

/// The `_embedded` object that holds a response's venues. Absent when a
/// search has no results.
#[derive(Deserialize, Default)]
struct RawEmbedded {
    #[serde(default)]
    venues: Vec<RawVenue>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPage {
    total_pages: u32,
}

/// A venue as the Discovery API returns it. Every field but `id` and
/// `name` is optional in practice.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawVenue {
    id: String,
    name: String,
    postal_code: Option<String>,
    city: Option<RawCity>,
    state: Option<RawState>,
    address: Option<RawAddress>,
    location: Option<RawLocation>,
}

#[derive(Deserialize)]
struct RawCity {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawState {
    state_code: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct RawAddress {
    line1: Option<String>,
}

/// Coordinates arrive as decimal strings.
#[derive(Deserialize)]
struct RawLocation {
    latitude: String,
    longitude: String,
}

impl RawVenue {
    /// Maps to a [`Venue`] with a `ticketmaster:` ID, or `None` when the
    /// venue has no usable coordinates (placeholders such as "Venue To Be
    /// Announced" often don't), since grpy can't place it on the map.
    fn into_venue(self) -> Option<Venue> {
        let location = self.location?;
        let lat = location.latitude.parse().ok()?;
        let lon = location.longitude.parse().ok()?;
        let id = VenueId::new(super::provider_id(), self.id).ok()?;

        let state = self.state.and_then(|state| state.state_code.or(state.name));
        let city_state = join([self.city.map(|city| city.name), state], ", ");
        let address = self.address.and_then(|a| a.line1).map(|line1| {
            let locality = join([Some(city_state.clone()), self.postal_code], " ");
            join([Some(line1), Some(locality)], ", ")
        });
        let label = if city_state.is_empty() {
            self.name.clone()
        } else {
            city_state
        };

        Some(Venue {
            id,
            name: self.name,
            address,
            location: Location { lat, lon, label },
        })
    }
}

/// Joins the present, non-empty parts with `sep`.
fn join<const N: usize>(parts: [Option<String>; N], sep: &str) -> String {
    parts
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(sep)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE_0: &str = include_str!("../../../tests/fixtures/ticketmaster/venues-page-0.json");
    const EMPTY: &str = include_str!("../../../tests/fixtures/ticketmaster/venues-empty.json");

    const EVENT: &str = include_str!("../../../tests/fixtures/ticketmaster/event.json");

    #[test]
    fn event_venue_is_the_first_embedded_venue() {
        let venue = parse_event_venue(EVENT).unwrap();

        assert_eq!(venue.id.to_string(), "ticketmaster:KovZpZHrlTest");
        assert_eq!(venue.name, "Hard Rock Live");
        assert_eq!(venue.location.label, "Hollywood, FL");
    }

    #[test]
    fn event_without_a_venue_is_an_error() {
        assert!(parse_event_venue(r#"{"id":"Z7r9jZ1ATest","name":"TBA"}"#).is_err());
    }

    #[test]
    fn venue_page_maps_venues_and_page_count() {
        let page = parse_venue_page(PAGE_0).unwrap();

        assert_eq!(page.total_pages, 2);
        assert_eq!(
            page.venues[0],
            Venue {
                id: "ticketmaster:KovZpZRevTest".parse().unwrap(),
                name: "Revolution Live".into(),
                address: Some("100 SW 3rd Ave, Fort Lauderdale, FL 33312".into()),
                location: Location {
                    lat: 26.1194,
                    lon: -80.1462,
                    label: "Fort Lauderdale, FL".into(),
                },
            }
        );
    }

    #[test]
    fn venues_without_coordinates_are_dropped() {
        let page = parse_venue_page(PAGE_0).unwrap();

        let names: Vec<&str> = page.venues.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, ["Revolution Live", "Culture Room"]);
    }

    #[test]
    fn page_without_results_is_empty() {
        let page = parse_venue_page(EMPTY).unwrap();

        assert_eq!(page, VenuePage::default());
    }

    #[test]
    fn malformed_body_is_an_error() {
        assert!(parse_venue_page("<html>gateway timeout</html>").is_err());
    }
}
