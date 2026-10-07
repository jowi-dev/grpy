//! Local state that survives between runs: followed venues, where their
//! events come from, cached events and what the user did with each one.
//!
//! Everything lives in one SQLite database, `$XDG_DATA_HOME/grpy/grpy.db`
//! (see [`Paths::store_file`](crate::config::Paths::store_file)). The
//! schema is versioned and migrated forward on [`Store::open`].

mod events;
mod migrations;
mod venues;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rusqlite::types::Type;
use rusqlite::{Connection, Row};

use crate::domain::VenueId;

pub use venues::VenueSource;

/// Why a [`Store`] operation failed.
#[derive(Debug)]
pub enum StoreError {
    /// The database was written by a newer grpy whose schema this build
    /// doesn't know.
    NewerSchema { found: u32, supported: u32 },
    /// The store has never seen this venue.
    UnknownVenue(VenueId),
    /// The database's directory could not be created.
    Io { path: PathBuf, source: io::Error },
    /// SQLite failed.
    Sqlite(rusqlite::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NewerSchema { found, supported } => write!(
                f,
                "database schema version {found} is newer than this grpy supports \
                 ({supported}); upgrade grpy"
            ),
            Self::UnknownVenue(id) => write!(f, "unknown venue `{id}`"),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Sqlite(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NewerSchema { .. } | Self::UnknownVenue(_) => None,
            Self::Io { source, .. } => Some(source),
            Self::Sqlite(err) => Some(err),
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Sqlite(err)
    }
}

/// Result type returned by [`Store`] methods.
pub type Result<T> = std::result::Result<T, StoreError>;

/// Handle to grpy's local database.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens the database at `path`, creating it and its parent
    /// directories if needed, and migrates it to the current schema.
    ///
    /// # Errors
    ///
    /// [`StoreError::NewerSchema`] if a newer grpy wrote the database,
    /// [`StoreError::Io`] if its directory can't be created, and
    /// [`StoreError::Sqlite`] if it can't be opened or migrated.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|source| StoreError::Io {
                path: dir.to_owned(),
                source,
            })?;
        }
        Self::init(Connection::open(path)?)
    }

    /// Opens a fresh database that lives only in memory, for tests.
    ///
    /// # Errors
    ///
    /// As [`Store::open`].
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        migrations::migrate(&mut conn)?;
        Ok(Self { conn })
    }

    /// The schema version the database is at.
    pub fn schema_version(&self) -> Result<u32> {
        migrations::version(&self.conn)
    }
}

/// Reads text column `idx` of `row` and parses it with [`FromStr`], for
/// IDs and URLs stored as their string form.
fn parsed<T>(row: &Row, idx: usize) -> rusqlite::Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let text: String = row.get(idx)?;
    text.parse()
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(idx, Type::Text, Box::new(err)))
}

/// As [`parsed`], for a nullable column.
fn parsed_opt<T>(row: &Row, idx: usize) -> rusqlite::Result<Option<T>>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match row.get_ref(idx)? {
        rusqlite::types::ValueRef::Null => Ok(None),
        _ => parsed(row, idx).map(Some),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_database_is_migrated_to_the_latest_schema() {
        let store = Store::open_in_memory().unwrap();

        assert_eq!(store.schema_version().unwrap(), migrations::LATEST);
    }

    #[test]
    fn reopening_an_existing_database_keeps_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/grpy.db");

        Store::open(&path).unwrap();
        let store = Store::open(&path).unwrap();

        assert_eq!(store.schema_version().unwrap(), migrations::LATEST);
    }

    #[test]
    fn database_from_a_newer_grpy_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grpy.db");
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", migrations::LATEST + 1)
            .unwrap();

        let err = Store::open(&path).unwrap_err();

        assert!(
            matches!(err, StoreError::NewerSchema { found, supported }
                if found == migrations::LATEST + 1 && supported == migrations::LATEST),
            "{err:?}"
        );
    }
}
