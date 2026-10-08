//! Schema migrations, applied in order and tracked in SQLite's
//! `user_version` pragma.
//!
//! Never edit a migration that has shipped; append a new one instead.

use rusqlite::Connection;

use super::{Result, StoreError};

/// SQL for each schema version: `MIGRATIONS[n]` takes the database from
/// version `n` to `n + 1`.
const MIGRATIONS: &[&str] = &[
    // 1: venues with their source info, cached events, per-event state.
    "
    CREATE TABLE venues (
        id                TEXT PRIMARY KEY,
        name              TEXT NOT NULL,
        address           TEXT,
        lat               REAL NOT NULL,
        lon               REAL NOT NULL,
        label             TEXT NOT NULL,
        followed          INTEGER NOT NULL DEFAULT 0,
        events_url        TEXT,
        feed_url          TEXT,
        extraction_tier   TEXT,
        ticketmaster_id   TEXT,
        -- Unix seconds of the last successful event fetch; NULL if never.
        events_fetched_at INTEGER
    );

    CREATE TABLE cached_events (
        id             TEXT PRIMARY KEY,
        venue_id       TEXT NOT NULL REFERENCES venues (id) ON DELETE CASCADE,
        title          TEXT NOT NULL,
        -- JSON array of strings, headliner first.
        artists        TEXT NOT NULL,
        -- RFC 3339 with the venue's offset, plus Unix seconds for ordering.
        starts_at      TEXT NOT NULL,
        starts_at_unix INTEGER NOT NULL,
        doors_at       TEXT,
        ticket_url     TEXT,
        source_url     TEXT NOT NULL
    );
    CREATE INDEX cached_events_by_venue ON cached_events (venue_id, starts_at_unix);

    -- Kept independently of cached_events so a calendar record outlives
    -- the cache.
    CREATE TABLE event_states (
        event_id        TEXT PRIMARY KEY,
        state           TEXT NOT NULL CHECK (state IN ('new', 'seen', 'dismissed', 'added')),
        google_event_id TEXT,
        CHECK ((state = 'added') = (google_event_id IS NOT NULL))
    );
    ",
];

/// The schema version a fully migrated database is at.
pub(super) const LATEST: u32 = MIGRATIONS.len() as u32;

/// The schema version `conn` is at.
pub(super) fn version(conn: &Connection) -> Result<u32> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

/// Applies every migration `conn` hasn't had yet, all in one transaction.
pub(super) fn migrate(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;
    let found = version(&tx)?;
    if found > LATEST {
        return Err(StoreError::NewerSchema {
            found,
            supported: LATEST,
        });
    }
    for (from, sql) in MIGRATIONS.iter().enumerate().skip(found as usize) {
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", from as u32 + 1)?;
    }
    tx.commit()?;
    Ok(())
}
