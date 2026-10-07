//! Where the Google refresh token is kept between runs.
//!
//! grpy prefers the OS keyring (Keychain, Windows Credential Manager or
//! the Secret Service) and falls back to a file readable only by the user
//! when no keyring is available, as on a headless machine.

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use super::AuthError;
use crate::config::Secret;

/// Keyring service name for grpy's entries.
pub const KEYRING_SERVICE: &str = "grpy";

/// Keyring user name for the Google refresh token.
pub const KEYRING_USER: &str = "google-refresh-token";

/// File name of the fallback store inside grpy's data directory.
pub const TOKEN_FILE_NAME: &str = "google-refresh-token";

/// Somewhere a refresh token can be saved and read back.
pub trait TokenStore {
    /// The saved token, or `None` if none has been saved.
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`] when the store can't be read.
    fn load(&self) -> Result<Option<Secret>, AuthError>;

    /// Saves `token`, replacing any earlier one, and says where it went,
    /// such as `"the OS keyring"` or a file path, for the user.
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`] when the store can't be written.
    fn save(&self, token: &Secret) -> Result<String, AuthError>;
}

/// The OS keyring, under [`KEYRING_SERVICE`] / [`KEYRING_USER`].
pub struct KeyringStore;

impl KeyringStore {
    fn entry() -> Result<keyring::Entry, AuthError> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(store_error)
    }
}

impl TokenStore for KeyringStore {
    fn load(&self) -> Result<Option<Secret>, AuthError> {
        match Self::entry()?.get_password() {
            Ok(token) => Ok(Some(Secret::new(token))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(store_error(err)),
        }
    }

    fn save(&self, token: &Secret) -> Result<String, AuthError> {
        Self::entry()?
            .set_password(token.expose())
            .map_err(store_error)?;
        Ok("the OS keyring".to_owned())
    }
}

fn store_error(err: impl std::fmt::Display) -> AuthError {
    AuthError::Store(err.to_string())
}

/// A file holding just the token, created with mode `0600` on Unix.
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    /// A store at `path`. Nothing is touched until the first save or load.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl TokenStore for FileStore {
    fn load(&self) -> Result<Option<Secret>, AuthError> {
        match fs::read_to_string(&self.path) {
            Ok(text) => Ok(Some(text.trim())
                .filter(|token| !token.is_empty())
                .map(Secret::new)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(self.error(err)),
        }
    }

    fn save(&self, token: &Secret) -> Result<String, AuthError> {
        self.write(token).map_err(|err| self.error(err))?;
        Ok(self.path.display().to_string())
    }
}

impl FileStore {
    fn write(&self, token: &Secret) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            options.mode(0o600);
            // `mode` only applies to new files; tighten an existing one
            // before writing the token into it.
            if self.path.exists() {
                fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))?;
            }
        }
        options.open(&self.path)?.write_all(token.expose().as_bytes())
    }

    fn error(&self, err: io::Error) -> AuthError {
        AuthError::Store(format!("{}: {err}", self.path.display()))
    }
}

/// Tries `primary` first and uses `fallback` when it fails.
///
/// Loading reads `fallback` when `primary` has no token or can't be read,
/// so a token saved to the fallback while the keyring was unavailable is
/// still found later.
pub struct FallbackStore<P, F> {
    primary: P,
    fallback: F,
}

impl<P, F> FallbackStore<P, F> {
    /// Combines two stores.
    pub fn new(primary: P, fallback: F) -> Self {
        Self { primary, fallback }
    }
}

impl<P: TokenStore, F: TokenStore> TokenStore for FallbackStore<P, F> {
    fn load(&self) -> Result<Option<Secret>, AuthError> {
        match self.primary.load() {
            Ok(Some(token)) => Ok(Some(token)),
            Ok(None) | Err(_) => self.fallback.load(),
        }
    }

    fn save(&self, token: &Secret) -> Result<String, AuthError> {
        self.primary
            .save(token)
            .or_else(|_| self.fallback.save(token))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn file_store_is_empty_before_the_first_save() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(FileStore::new(dir.path().join("token")).load(), Ok(None));
    }

    #[test]
    fn file_store_round_trips_a_token_creating_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grpy").join(TOKEN_FILE_NAME);
        let store = FileStore::new(&path);

        let location = store.save(&Secret::new("1//first")).unwrap();
        store.save(&Secret::new("1//second")).unwrap();

        assert_eq!(location, path.display().to_string());
        assert_eq!(store.load(), Ok(Some(Secret::new("1//second"))));
    }

    #[cfg(unix)]
    #[test]
    fn file_store_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        FileStore::new(&path).save(&Secret::new("1//token")).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn file_store_reads_an_empty_file_as_no_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        fs::write(&path, "\n").unwrap();

        assert_eq!(FileStore::new(&path).load(), Ok(None));
    }

    /// An in-memory store that can be made to fail.
    #[derive(Default)]
    struct FakeStore {
        token: RefCell<Option<Secret>>,
        broken: bool,
    }

    impl FakeStore {
        fn broken() -> Self {
            Self {
                broken: true,
                ..Self::default()
            }
        }

        fn holding(token: &str) -> Self {
            Self {
                token: RefCell::new(Some(Secret::new(token))),
                broken: false,
            }
        }
    }

    impl TokenStore for &FakeStore {
        fn load(&self) -> Result<Option<Secret>, AuthError> {
            if self.broken {
                return Err(AuthError::Store("no keyring".into()));
            }
            Ok(self.token.borrow().clone())
        }

        fn save(&self, token: &Secret) -> Result<String, AuthError> {
            if self.broken {
                return Err(AuthError::Store("no keyring".into()));
            }
            *self.token.borrow_mut() = Some(token.clone());
            Ok(format!("fake{:p}", *self))
        }
    }

    #[test]
    fn fallback_store_saves_to_the_primary_when_it_works() {
        let (primary, fallback) = (FakeStore::default(), FakeStore::default());

        let location = FallbackStore::new(&primary, &fallback)
            .save(&Secret::new("1//token"))
            .unwrap();

        assert_eq!(location, format!("fake{:p}", &primary));
        assert_eq!(*primary.token.borrow(), Some(Secret::new("1//token")));
        assert_eq!(*fallback.token.borrow(), None);
    }

    #[test]
    fn fallback_store_saves_to_the_fallback_when_the_primary_fails() {
        let (primary, fallback) = (FakeStore::broken(), FakeStore::default());

        let location = FallbackStore::new(&primary, &fallback)
            .save(&Secret::new("1//token"))
            .unwrap();

        assert_eq!(location, format!("fake{:p}", &fallback));
        assert_eq!(*fallback.token.borrow(), Some(Secret::new("1//token")));
    }

    #[test]
    fn fallback_store_prefers_the_primary_token() {
        let (primary, fallback) = (FakeStore::holding("1//keyring"), FakeStore::holding("1//file"));

        assert_eq!(
            FallbackStore::new(&primary, &fallback).load(),
            Ok(Some(Secret::new("1//keyring")))
        );
    }

    #[test]
    fn fallback_store_reads_the_fallback_when_the_primary_is_empty_or_broken() {
        let fallback = FakeStore::holding("1//file");

        for primary in [FakeStore::default(), FakeStore::broken()] {
            assert_eq!(
                FallbackStore::new(&primary, &fallback).load(),
                Ok(Some(Secret::new("1//file")))
            );
        }
    }

    #[test]
    fn fallback_store_reports_the_fallback_error_when_both_fail() {
        let (primary, fallback) = (FakeStore::broken(), FakeStore::broken());
        let store = FallbackStore::new(&primary, &fallback);

        assert!(matches!(store.load(), Err(AuthError::Store(_))));
        assert!(matches!(
            store.save(&Secret::new("1//token")),
            Err(AuthError::Store(_))
        ));
    }
}
