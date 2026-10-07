//! Followed venues.

use rusqlite::{OptionalExtension, Row, params};

use crate::domain::{Location, Venue, VenueId};

use super::{Result, Store, parsed};

impl Store {
    /// Follows `venue`, saving its details. Following an already-followed
    /// venue refreshes its details.
    pub fn follow(&self, venue: &Venue) -> Result<()> {
        self.conn.execute(
            "INSERT INTO venues (id, name, address, lat, lon, label, followed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)
             ON CONFLICT (id) DO UPDATE SET
                 name = excluded.name, address = excluded.address,
                 lat = excluded.lat, lon = excluded.lon, label = excluded.label,
                 followed = 1",
            params![
                venue.id.to_string(),
                venue.name,
                venue.address,
                venue.location.lat,
                venue.location.lon,
                venue.location.label,
            ],
        )?;
        Ok(())
    }

    /// Stops following `id`. Returns whether it was followed.
    ///
    /// The venue's source info and event states are kept, so following it
    /// again picks up where it left off.
    pub fn unfollow(&self, id: &VenueId) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE venues SET followed = 0 WHERE id = ?1 AND followed = 1",
            [id.to_string()],
        )?;
        Ok(changed > 0)
    }

    /// Whether `id` is followed.
    pub fn is_followed(&self, id: &VenueId) -> Result<bool> {
        let followed = self
            .conn
            .query_row(
                "SELECT followed FROM venues WHERE id = ?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(followed.unwrap_or(false))
    }

    /// Every followed venue, by name.
    pub fn followed_venues(&self) -> Result<Vec<Venue>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, address, lat, lon, label FROM venues
             WHERE followed = 1 ORDER BY name, id",
        )?;
        let venues = stmt.query_map([], venue_from_row)?.collect::<rusqlite::Result<_>>()?;
        Ok(venues)
    }
}

fn venue_from_row(row: &Row) -> rusqlite::Result<Venue> {
    Ok(Venue {
        id: parsed(row, 0)?,
        name: row.get(1)?,
        address: row.get(2)?,
        location: Location {
            lat: row.get(3)?,
            lon: row.get(4)?,
            label: row.get(5)?,
        },
    })
}
#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn venue(id: &str, name: &str) -> Venue {
        Venue {
            id: id.parse().unwrap(),
            name: name.into(),
            address: Some("100 SW 3rd Ave, Fort Lauderdale, FL 33312".into()),
            location: Location {
                lat: 26.1194,
                lon: -80.1462,
                label: "Fort Lauderdale, FL".into(),
            },
        }
    }

    #[test]
    fn follow_unfollow_round_trips() {
        let store = Store::open_in_memory().unwrap();
        let revolution = venue("ics:revolution-live", "Revolution Live");

        store.follow(&revolution).unwrap();
        assert!(store.is_followed(&revolution.id).unwrap());
        assert_eq!(store.followed_venues().unwrap(), std::slice::from_ref(&revolution));

        assert!(store.unfollow(&revolution.id).unwrap());
        assert!(!store.is_followed(&revolution.id).unwrap());
        assert!(store.followed_venues().unwrap().is_empty());
    }

    #[test]
    fn followed_venues_survive_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grpy.db");
        let culture_room = Venue {
            address: None,
            ..venue("osm:node/42", "Culture Room")
        };

        Store::open(&path).unwrap().follow(&culture_room).unwrap();

        assert_eq!(
            Store::open(&path).unwrap().followed_venues().unwrap(),
            [culture_room]
        );
    }

    #[test]
    fn followed_venues_are_sorted_by_name() {
        let store = Store::open_in_memory().unwrap();
        store.follow(&venue("ics:b", "Revolution Live")).unwrap();
        store.follow(&venue("ics:a", "Culture Room")).unwrap();

        let names: Vec<_> = store
            .followed_venues()
            .unwrap()
            .into_iter()
            .map(|v| v.name)
            .collect();
        assert_eq!(names, ["Culture Room", "Revolution Live"]);
    }

    #[test]
    fn following_again_updates_details() {
        let store = Store::open_in_memory().unwrap();
        store.follow(&venue("ics:rev", "Revolution")).unwrap();
        let renamed = venue("ics:rev", "Revolution Live");

        store.follow(&renamed).unwrap();

        assert_eq!(store.followed_venues().unwrap(), [renamed]);
    }

    #[test]
    fn unfollowing_an_unknown_venue_is_a_no_op() {
        let store = Store::open_in_memory().unwrap();

        assert!(!store.unfollow(&"ics:nowhere".parse().unwrap()).unwrap());
    }

    #[test]
    fn unfollowing_twice_reports_the_second_as_a_no_op() {
        let store = Store::open_in_memory().unwrap();
        let revolution = venue("ics:revolution-live", "Revolution Live");
        store.follow(&revolution).unwrap();

        assert!(store.unfollow(&revolution.id).unwrap());
        assert!(!store.unfollow(&revolution.id).unwrap());
    }
}
