//! User configuration and secrets.
//!
//! Config lives in `$XDG_CONFIG_HOME/grpy/config.toml` (see [`Paths`]);
//! [`load_or_init`] reads it, or writes a commented example on first run.
//! Provider keys can be overridden from the environment, and every secret
//! is held in a [`Secret`] so it never shows up in logs.

mod file;
mod paths;
mod secret;
mod settings;

pub use file::{EXAMPLE_CONFIG, LoadError, load_or_init};
pub use paths::Paths;
pub use secret::Secret;
pub use settings::{
    Config, ConfigError, DEFAULT_CALENDAR_ID, DEFAULT_RADIUS_MILES, GOOGLE_CLIENT_ID_ENV,
    GOOGLE_CLIENT_SECRET_ENV, GoogleClient, Home, HomePlace, ProviderKeys,
    TICKETMASTER_KEY_ENV, Warning,
};
