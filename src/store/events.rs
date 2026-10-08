//! Cached events per venue, so grpy doesn't refetch on every launch.

use std::time::Duration;

use chrono::{DateTime, FixedOffset, TimeDelta, Utc};
use rusqlite::types::Type;
use rusqlite::{OptionalExtension, Row, params};
use url::Url;

use crate::domain::{Event, VenueId};

use super::{Result, Store, StoreError, parsed, parsed_opt};

impl Store {
    /// Replaces `venue`'s cached events with `events`, fetched at
    /// `fetched_at`. `events` should all be at `venue`; an empty list
    /// records that the venue has no upcoming shows.
    ///
    /// # Errors
    ///
    /// [`StoreError::UnknownVenue`] if the store has never seen `venue`.
    pub fn cache_events(
        &mut self,
        venue: &VenueId,
        events: &[Event],
        fetched_at: DateTime<Utc>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        let changed = tx.execute(
            "UPDATE venues SET events_fetched_at = ?2 WHERE id = ?1",
            params![venue.to_string(), fetched_at.timestamp()],
        )?;
        if changed == 0 {
            return Err(StoreError::UnknownVenue(venue.clone()));
        }
        tx.execute(
            "DELETE FROM cached_events WHERE venue_id = ?1",
            [venue.to_string()],
        )?;
        {
            let mut insert = tx.prepare(
                "INSERT OR REPLACE INTO cached_events (id, venue_id, title, artists,
                     starts_at, starts_at_unix, doors_at, ticket_url, source_url)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for event in events {
                insert.execute(params![
                    event.id.to_string(),
                    event.venue_id.to_string(),
                    event.title,
                    serde_json::to_string(&event.artists)
                        .expect("a list of strings always serializes"),
                    event.starts_at.to_rfc3339(),
                    event.starts_at.timestamp(),
                    event.doors_at.map(|at| at.to_rfc3339()),
                    event.ticket_url.as_ref().map(Url::as_str),
                    event.source_url.as_str(),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// `venue`'s cached events, earliest first, if they were fetched less
    /// than `ttl` before `now`. `None` means the cache is stale or the
    /// venue was never fetched, so the caller should fetch again.
    pub fn cached_events(
        &self,
        venue: &VenueId,
        now: DateTime<Utc>,
        ttl: Duration,
    ) -> Result<Option<Vec<Event>>> {
        let fetched_at: Option<i64> = self
            .conn
            .query_row(
                "SELECT events_fetched_at FROM venues WHERE id = ?1",
                [venue.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let ttl = TimeDelta::from_std(ttl).unwrap_or(TimeDelta::MAX);
        let fresh = fetched_at
            .and_then(|secs| DateTime::from_timestamp(secs, 0))
            .is_some_and(|fetched_at| now.signed_duration_since(fetched_at) < ttl);
        if !fresh {
            return Ok(None);
        }

        let mut stmt = self.conn.prepare(
            "SELECT id, venue_id, title, artists, starts_at, doors_at, ticket_url, source_url
             FROM cached_events WHERE venue_id = ?1 ORDER BY starts_at_unix, id",
        )?;
        let events = stmt
            .query_map([venue.to_string()], event_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Some(events))
    }
}

fn event_from_row(row: &Row) -> rusqlite::Result<Event> {
    let artists: String = row.get(3)?;
    Ok(Event {
        id: parsed(row, 0)?,
        venue_id: parsed(row, 1)?,
        title: row.get(2)?,
        artists: serde_json::from_str(&artists).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(3, Type::Text, Box::new(err))
        })?,
        starts_at: parsed::<DateTime<FixedOffset>>(row, 4)?,
        doors_at: parsed_opt::<DateTime<FixedOffset>>(row, 5)?,
        ticket_url: parsed_opt(row, 6)?,
        source_url: parsed(row, 7)?,
    })
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::store::StoreError;
    use crate::store::venues::tests::venue;

    const HOUR: Duration = Duration::from_secs(3600);

    fn noon() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 7, 12, 0, 0).unwrap()
    }

    fn show(venue: &VenueId, slug: &str, starts_at: &str) -> Event {
        let url = format!("https://www.jointherevolution.net/events/{slug}/");
        Event {
            id: format!("ics:{url}").parse().unwrap(),
            venue_id: venue.clone(),
            title: slug.into(),
            artists: vec![slug.into(), "Opener".into()],
            starts_at: DateTime::parse_from_rfc3339(starts_at).unwrap(),
            doors_at: Some(DateTime::parse_from_rfc3339(starts_at).unwrap() - HOUR),
            ticket_url: Some("https://www.ticketweb.com/event/123".parse().unwrap()),
            source_url: url.parse().unwrap(),
        }
    }

    fn followed_store() -> (Store, VenueId) {
        let store = Store::open_in_memory().unwrap();
        let revolution = venue("ics:revolution-live", "Revolution Live");
        store.follow(&revolution).unwrap();
        (store, revolution.id)
    }

    #[test]
    fn fresh_cache_round_trips_events_earliest_first() {
        let (mut store, id) = followed_store();
        let later = show(&id, "later", "2026-11-01T20:00:00-04:00");
        // Earlier instant despite the later wall-clock time in another offset.
        let sooner = Event {
            doors_at: None,
            ticket_url: None,
            artists: vec![],
            ..show(&id, "sooner", "2026-11-01T20:30:00-03:00")
        };

        store
            .cache_events(&id, &[later.clone(), sooner.clone()], noon())
            .unwrap();

        assert_eq!(
            store.cached_events(&id, noon() + HOUR, 2 * HOUR).unwrap(),
            Some(vec![sooner, later])
        );
    }

    #[test]
    fn cache_older_than_ttl_is_stale() {
        let (mut store, id) = followed_store();
        let event = show(&id, "band", "2026-11-01T20:00:00-04:00");
        store.cache_events(&id, &[event], noon()).unwrap();

        assert_eq!(store.cached_events(&id, noon() + HOUR, HOUR).unwrap(), None);
    }

    #[test]
    fn never_fetched_venue_has_no_cache() {
        let (store, id) = followed_store();

        assert_eq!(store.cached_events(&id, noon(), HOUR).unwrap(), None);
    }

    #[test]
    fn empty_fetch_is_cached_as_no_shows() {
        let (mut store, id) = followed_store();
        store.cache_events(&id, &[], noon()).unwrap();

        assert_eq!(
            store.cached_events(&id, noon(), HOUR).unwrap(),
            Some(vec![])
        );
    }

    #[test]
    fn refetch_replaces_the_cache() {
        let (mut store, id) = followed_store();
        let old = show(&id, "cancelled", "2026-11-01T20:00:00-04:00");
        let new = show(&id, "announced", "2026-11-02T20:00:00-04:00");
        store.cache_events(&id, &[old], noon()).unwrap();

        store
            .cache_events(&id, std::slice::from_ref(&new), noon() + HOUR)
            .unwrap();

        assert_eq!(
            store.cached_events(&id, noon() + HOUR, HOUR).unwrap(),
            Some(vec![new])
        );
    }

    #[test]
    fn caching_for_an_unknown_venue_fails() {
        let mut store = Store::open_in_memory().unwrap();
        let id: VenueId = "ics:nowhere".parse().unwrap();

        let err = store.cache_events(&id, &[], noon()).unwrap_err();

        assert!(matches!(err, StoreError::UnknownVenue(_)), "{err:?}");
    }
}
