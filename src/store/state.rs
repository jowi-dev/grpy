//! What the user has done with each event.

use rusqlite::{OptionalExtension, params};

use crate::domain::EventId;

use super::{Result, Store, StoreError};

/// What the user has done with an event.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EventState {
    /// Not yet shown to the user.
    #[default]
    New,
    /// Shown to the user, who hasn't acted on it.
    Seen,
    /// Hidden by the user.
    Dismissed,
    /// Added to Google Calendar as `google_event_id`. Final: see
    /// [`Store::set_event_state`].
    AddedToCalendar { google_event_id: String },
}

impl Store {
    /// The state of `event`; [`EventState::New`] if it has none yet.
    pub fn event_state(&self, event: &EventId) -> Result<EventState> {
        let row: Option<(String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT state, google_event_id FROM event_states WHERE event_id = ?1",
                [event.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(match row {
            None => EventState::New,
            Some((state, google_event_id)) => match (state.as_str(), google_event_id) {
                ("seen", _) => EventState::Seen,
                ("dismissed", _) => EventState::Dismissed,
                ("added", Some(google_event_id)) => EventState::AddedToCalendar { google_event_id },
                // The schema's CHECK constraints rule out anything else.
                _ => EventState::New,
            },
        })
    }

    /// Records `state` for `event`.
    ///
    /// Once an event is [`EventState::AddedToCalendar`] its state can't
    /// change, so the same show is never added to the calendar twice.
    ///
    /// # Errors
    ///
    /// [`StoreError::AlreadyAdded`] if `event` is already on the calendar.
    pub fn set_event_state(&self, event: &EventId, state: &EventState) -> Result<()> {
        let (name, google_event_id) = match state {
            EventState::New => ("new", None),
            EventState::Seen => ("seen", None),
            EventState::Dismissed => ("dismissed", None),
            EventState::AddedToCalendar { google_event_id } => ("added", Some(google_event_id)),
        };
        let changed = self.conn.execute(
            "INSERT INTO event_states (event_id, state, google_event_id) VALUES (?1, ?2, ?3)
             ON CONFLICT (event_id) DO UPDATE
                 SET state = excluded.state, google_event_id = excluded.google_event_id
                 WHERE event_states.state != 'added'",
            params![event.to_string(), name, google_event_id],
        )?;
        if changed > 0 {
            return Ok(());
        }
        match self.event_state(event)? {
            EventState::AddedToCalendar { google_event_id } => Err(StoreError::AlreadyAdded {
                event: event.clone(),
                google_event_id,
            }),
            _ => unreachable!("an upsert only skips events already added"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::StoreError;

    fn event() -> EventId {
        "ics:https://www.jointherevolution.net/events/some-band/"
            .parse()
            .unwrap()
    }

    fn added(google_event_id: &str) -> EventState {
        EventState::AddedToCalendar {
            google_event_id: google_event_id.into(),
        }
    }

    #[test]
    fn unknown_event_is_new() {
        let store = Store::open_in_memory().unwrap();

        assert_eq!(store.event_state(&event()).unwrap(), EventState::New);
    }

    #[test]
    fn every_state_round_trips() {
        for state in [
            EventState::New,
            EventState::Seen,
            EventState::Dismissed,
            added("gcal-123"),
        ] {
            let store = Store::open_in_memory().unwrap();

            store.set_event_state(&event(), &state).unwrap();

            assert_eq!(store.event_state(&event()).unwrap(), state);
        }
    }

    #[test]
    fn state_can_change_until_added() {
        let store = Store::open_in_memory().unwrap();
        store
            .set_event_state(&event(), &EventState::Dismissed)
            .unwrap();

        store.set_event_state(&event(), &EventState::Seen).unwrap();

        assert_eq!(store.event_state(&event()).unwrap(), EventState::Seen);
    }

    #[test]
    fn adding_twice_is_refused_and_keeps_the_first_calendar_event() {
        let store = Store::open_in_memory().unwrap();
        store.set_event_state(&event(), &added("gcal-123")).unwrap();

        let err = store
            .set_event_state(&event(), &added("gcal-456"))
            .unwrap_err();

        assert!(
            matches!(&err, StoreError::AlreadyAdded { event: got, google_event_id }
                if *got == event() && google_event_id == "gcal-123"),
            "{err:?}"
        );
        assert_eq!(store.event_state(&event()).unwrap(), added("gcal-123"));
    }

    #[test]
    fn added_event_cannot_be_dismissed() {
        let store = Store::open_in_memory().unwrap();
        store.set_event_state(&event(), &added("gcal-123")).unwrap();

        let err = store
            .set_event_state(&event(), &EventState::Dismissed)
            .unwrap_err();

        assert!(matches!(err, StoreError::AlreadyAdded { .. }), "{err:?}");
        assert_eq!(store.event_state(&event()).unwrap(), added("gcal-123"));
    }

    #[test]
    fn state_survives_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grpy.db");

        Store::open(&path)
            .unwrap()
            .set_event_state(&event(), &added("gcal-123"))
            .unwrap();

        assert_eq!(
            Store::open(&path).unwrap().event_state(&event()).unwrap(),
            added("gcal-123")
        );
    }
}
