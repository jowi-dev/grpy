//! Reading `config.toml` from disk, writing an example on first run.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::{Config, ConfigError};

/// The commented config written on first run.
pub const EXAMPLE_CONFIG: &str = include_str!("example.toml");

/// Why [`load_or_init`] produced no config.
#[derive(Debug)]
pub enum LoadError {
    /// No config existed, so an example was written to `path`. The user
    /// needs to edit it before grpy can run.
    ExampleWritten { path: PathBuf },
    /// The file could not be read or the example could not be written.
    Io { path: PathBuf, source: io::Error },
    /// The file exists but is invalid or incomplete.
    Invalid { path: PathBuf, source: ConfigError },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExampleWritten { path } => write!(
                f,
                "wrote an example config to {}; set your home location there and run grpy again",
                path.display()
            ),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Invalid { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ExampleWritten { .. } => None,
            Self::Io { source, .. } => Some(source),
            Self::Invalid { source, .. } => Some(source),
        }
    }
}

/// Loads the config at `path`, applying environment overrides as
/// [`Config::parse`] does.
///
/// If the file doesn't exist, writes [`EXAMPLE_CONFIG`] there (creating
/// parent directories, mode `0600` on Unix since it will hold secrets) and
/// returns [`LoadError::ExampleWritten`].
///
/// # Errors
///
/// [`LoadError::ExampleWritten`] on first run, [`LoadError::Io`] when the
/// file can't be read or the example can't be written, and
/// [`LoadError::Invalid`] when the file doesn't parse or lacks a required
/// key.
pub fn load_or_init(
    path: &Path,
    env: impl Fn(&str) -> Option<String>,
) -> Result<Config, LoadError> {
    let io_error = |source| LoadError::Io {
        path: path.to_owned(),
        source,
    };

    match fs::read_to_string(path) {
        Ok(text) => Config::parse(&text, env).map_err(|source| LoadError::Invalid {
            path: path.to_owned(),
            source,
        }),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            write_example(path).map_err(io_error)?;
            Err(LoadError::ExampleWritten {
                path: path.to_owned(),
            })
        }
        Err(err) => Err(io_error(err)),
    }
}

fn write_example(path: &Path) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(EXAMPLE_CONFIG.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DEFAULT_CALENDAR_ID, DEFAULT_RADIUS_MILES, HomePlace};

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn example_needs_only_a_home_to_load() {
        let err = Config::parse(EXAMPLE_CONFIG, no_env).unwrap_err();
        assert!(
            matches!(
                err,
                ConfigError::MissingKey {
                    key: "home.address",
                    ..
                }
            ),
            "{err:?}"
        );

        let edited = EXAMPLE_CONFIG.replace("# address =", "address =");
        let config = Config::parse(&edited, no_env).unwrap();

        assert_eq!(
            config.home.place,
            HomePlace::Address("Fort Lauderdale, FL".into())
        );
        assert_eq!(config.home.radius_miles, DEFAULT_RADIUS_MILES);
        assert_eq!(config.calendar_id, DEFAULT_CALENDAR_ID);
        assert_eq!(config.providers.ticketmaster, None);
    }

    #[test]
    fn example_with_every_line_uncommented_is_valid() {
        let edited = EXAMPLE_CONFIG
            .lines()
            .map(|line| match line.strip_prefix("# ") {
                Some(rest) if rest.contains(" = ") => rest,
                _ => line,
            })
            .collect::<Vec<_>>()
            .join("\n");

        Config::parse(&edited, no_env).unwrap();
    }

    #[test]
    fn first_run_writes_the_example_and_asks_for_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grpy/config.toml");

        let err = load_or_init(&path, no_env).unwrap_err();

        assert!(matches!(&err, LoadError::ExampleWritten { path: p } if *p == path));
        assert!(err.to_string().contains(&path.display().to_string()));
        assert_eq!(fs::read_to_string(&path).unwrap(), EXAMPLE_CONFIG);
    }

    #[cfg(unix)]
    #[test]
    fn example_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        load_or_init(&path, no_env).unwrap_err();

        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn loads_an_existing_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[home]\naddress = \"33301\"\n").unwrap();

        let config = load_or_init(&path, no_env).unwrap();

        assert_eq!(config.home.place, HomePlace::Address("33301".into()));
    }

    #[test]
    fn invalid_config_error_names_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[home]\n").unwrap();

        let err = load_or_init(&path, no_env).unwrap_err();

        assert!(matches!(err, LoadError::Invalid { .. }), "{err:?}");
        let shown = err.to_string();
        assert!(shown.contains(&path.display().to_string()), "{shown}");
        assert!(shown.contains("home.address"), "{shown}");
    }
}
