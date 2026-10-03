//! In-memory provider for tests and TUI development.

use crate::domain::{DateRange, Event, Location, Venue, VenueId};

use super::{Error, EventProvider, Result};

/// An [`EventProvider`] that serves a fixed set of venues and events from
/// memory, with no network access.
///
/// ```
/// # use grpy::domain::{Location, Venue};
/// # use grpy::provider::{EventProvider, FakeProvider};
/// # tokio::runtime::Runtime::new().unwrap().block_on(async {
/// let here = Location { lat: 26.1194, lon: -80.1462, label: "Fort Lauderdale, FL".into() };
/// let provider = FakeProvider::new().with_venue(Venue {
///     id: "fake:revolution-live".parse().unwrap(),
///     name: "Revolution Live".into(),
///     address: None,
///     location: here.clone(),
/// });
///
/// let venues = provider.venues_near(&here, 10).await.unwrap();
/// assert_eq!(venues[0].name, "Revolution Live");
/// # });
/// ```
#[derive(Debug, Clone, Default)]
pub struct FakeProvider {
    venues: Vec<Venue>,
    events: Vec<Event>,
}

impl FakeProvider {
    /// A provider with no venues or events.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a venue that [`venues_near`](EventProvider::venues_near) can
    /// return.
    pub fn with_venue(mut self, venue: Venue) -> Self {
        self.venues.push(venue);
        self
    }

    /// Adds an event. It is only returned by
    /// [`upcoming_events`](EventProvider::upcoming_events) if its venue was
    /// also added with [`with_venue`](Self::with_venue).
    pub fn with_event(mut self, event: Event) -> Self {
        self.events.push(event);
        self
    }
}

impl EventProvider for FakeProvider {
    async fn venues_near(&self, loc: &Location, radius_km: u32) -> Result<Vec<Venue>> {
        let radius_km = f64::from(radius_km);
        let mut nearby: Vec<(f64, &Venue)> = self
            .venues
            .iter()
            .map(|venue| (loc.distance_km(&venue.location), venue))
            .filter(|(km, _)| *km <= radius_km)
            .collect();
        nearby.sort_by(|(a, _), (b, _)| a.total_cmp(b));
        Ok(nearby.into_iter().map(|(_, venue)| venue.clone()).collect())
    }

    async fn upcoming_events(&self, venue: &VenueId, window: DateRange) -> Result<Vec<Event>> {
        if !self.venues.iter().any(|v| v.id == *venue) {
            return Err(Error::UnknownVenue(venue.clone()));
        }
        let mut events: Vec<Event> = self
            .events
            .iter()
            .filter(|e| e.venue_id == *venue && window.contains(&e.starts_at))
            .cloned()
            .collect();
        events.sort_by_key(|e| e.starts_at);
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::*;

    fn at(lat: f64, lon: f64) -> Location {
        Location {
            lat,
            lon,
            label: String::new(),
        }
    }

    fn venue(id: &str, location: Location) -> Venue {
        Venue {
            id: id.parse().unwrap(),
            name: id.into(),
            address: None,
            location,
        }
    }

    fn event(id: &str, venue: &Venue, starts_at: &str) -> Event {
        Event {
            id: id.parse().unwrap(),
            venue_id: venue.id.clone(),
            title: id.into(),
            artists: vec![],
            starts_at: DateTime::parse_from_rfc3339(starts_at).unwrap(),
            doors_at: None,
            ticket_url: None,
            source_url: "https://example.com/show".parse().unwrap(),
        }
    }

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().to_utc()
    }

    fn october() -> DateRange {
        DateRange::new(utc("2026-10-01T00:00:00Z"), utc("2026-11-01T00:00:00Z")).unwrap()
    }

    fn fort_lauderdale() -> Location {
        at(26.1224, -80.1373)
    }

    fn ids<T, I: ToString>(items: &[T], id: impl Fn(&T) -> &I) -> Vec<String> {
        items.iter().map(|item| id(item).to_string()).collect()
    }

    #[tokio::test]
    async fn venues_near_returns_venues_within_radius_nearest_first() {
        let provider = FakeProvider::new()
            .with_venue(venue("fake:miami", at(25.7617, -80.1918))) // ~40 km
            .with_venue(venue("fake:orlando", at(28.5384, -81.3789))) // ~290 km
            .with_venue(venue("fake:downtown", at(26.1194, -80.1462))); // ~1 km

        let venues = provider.venues_near(&fort_lauderdale(), 50).await.unwrap();

        assert_eq!(ids(&venues, |v| &v.id), ["fake:downtown", "fake:miami"]);
    }

    #[tokio::test]
    async fn venues_near_with_nothing_in_range_is_empty() {
        let provider = FakeProvider::new().with_venue(venue("fake:orlando", at(28.5384, -81.3789)));

        let venues = provider.venues_near(&fort_lauderdale(), 50).await.unwrap();

        assert!(venues.is_empty());
    }

    #[tokio::test]
    async fn upcoming_events_filters_by_venue_and_window_earliest_first() {
        let club = venue("fake:club", fort_lauderdale());
        let other = venue("fake:other", fort_lauderdale());
        let provider = FakeProvider::new()
            .with_venue(club.clone())
            .with_venue(other.clone())
            .with_event(event("fake:late", &club, "2026-10-20T20:00:00-04:00"))
            .with_event(event("fake:early", &club, "2026-10-03T20:00:00-04:00"))
            .with_event(event("fake:elsewhere", &other, "2026-10-10T20:00:00-04:00"))
            .with_event(event("fake:november", &club, "2026-11-05T20:00:00-05:00"));

        let events = provider.upcoming_events(&club.id, october()).await.unwrap();

        assert_eq!(ids(&events, |e| &e.id), ["fake:early", "fake:late"]);
    }

    #[tokio::test]
    async fn upcoming_events_for_known_venue_without_shows_is_empty() {
        let club = venue("fake:club", fort_lauderdale());
        let provider = FakeProvider::new().with_venue(club.clone());

        let events = provider.upcoming_events(&club.id, october()).await.unwrap();

        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn upcoming_events_for_unknown_venue_is_an_error() {
        let provider = FakeProvider::new();
        let missing: VenueId = "fake:nowhere".parse().unwrap();

        let err = provider
            .upcoming_events(&missing, october())
            .await
            .unwrap_err();

        assert!(matches!(&err, Error::UnknownVenue(id) if *id == missing));
        assert_eq!(err.to_string(), "unknown venue `fake:nowhere`");
    }

    #[tokio::test]
    async fn provider_futures_can_be_spawned_on_the_runtime() {
        async fn count_venues(provider: impl EventProvider + Send + 'static) -> usize {
            tokio::spawn(async move {
                provider
                    .venues_near(&fort_lauderdale(), 10)
                    .await
                    .unwrap()
                    .len()
            })
            .await
            .unwrap()
        }

        let provider = FakeProvider::new().with_venue(venue("fake:club", fort_lauderdale()));

        assert_eq!(count_venues(provider).await, 1);
    }
}
