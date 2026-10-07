//! The parsed `config.toml`.

use std::fmt;

use serde::Deserialize;

use super::Secret;

/// Environment variable that overrides `providers.ticketmaster_key`.
pub const TICKETMASTER_KEY_ENV: &str = "GRPY_TICKETMASTER_KEY";

/// Environment variable that overrides `google.client_id`.
pub const GOOGLE_CLIENT_ID_ENV: &str = "GRPY_GOOGLE_CLIENT_ID";

/// Environment variable that overrides `google.client_secret`.
pub const GOOGLE_CLIENT_SECRET_ENV: &str = "GRPY_GOOGLE_CLIENT_SECRET";

/// Search radius used when `home.radius_miles` is not set.
pub const DEFAULT_RADIUS_MILES: f64 = 25.0;

/// Calendar used when `calendar.calendar_id` is not set.
pub const DEFAULT_CALENDAR_ID: &str = "primary";

/// Everything grpy reads from `config.toml` and the environment.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// API keys for event providers.
    pub providers: ProviderKeys,
    /// Where to search from.
    pub home: Home,
    /// Google Calendar ID that events are added to.
    pub calendar_id: String,
    /// The OAuth client grpy signs in to Google with, if configured.
    /// `grpy auth google` needs it.
    pub google: Option<GoogleClient>,
}

/// An OAuth client of type "Desktop app" from the Google Cloud console.
#[derive(Debug, Clone, PartialEq)]
pub struct GoogleClient {
    /// The client ID, ending in `.apps.googleusercontent.com`.
    pub client_id: String,
    /// The client secret. Google doesn't treat a desktop client's secret
    /// as confidential, but grpy still keeps it out of logs.
    pub client_secret: Secret,
}

/// API keys for event providers. All optional: a provider without a key
/// is skipped, and [`Config::warnings`] says what that costs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProviderKeys {
    /// Ticketmaster Discovery API key.
    pub ticketmaster: Option<Secret>,
}

/// The search origin and how far from it to look.
#[derive(Debug, Clone, PartialEq)]
pub struct Home {
    /// Where home is.
    pub place: HomePlace,
    /// Search radius around home, in miles.
    pub radius_miles: f64,
}

impl Home {
    /// [`radius_miles`](Self::radius_miles) in whole kilometres, as
    /// providers take it. Rounds to the nearest kilometre; a negative
    /// radius becomes 0.
    pub fn radius_km(&self) -> u32 {
        const KM_PER_MILE: f64 = 1.609_344;
        // `as` saturates, so negatives and NaN become 0.
        (self.radius_miles * KM_PER_MILE).round() as u32
    }
}

/// Home as configured, before any geocoding.
#[derive(Debug, Clone, PartialEq)]
pub enum HomePlace {
    /// Free-form address, city or ZIP code, to be geocoded.
    Address(String),
    /// Explicit coordinates in decimal degrees (WGS 84).
    Coordinates { lat: f64, lon: f64 },
}

/// Something worth telling the user about a config that still loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// No Ticketmaster key, so Ticketmaster-ticketed venues are skipped.
    MissingTicketmasterKey,
}

/// Why a config could not be loaded.
#[derive(Debug)]
pub enum ConfigError {
    /// The file is not valid TOML or has a value of the wrong type.
    ///
    /// Only the parser's message and the line number are kept, never the
    /// offending source text, so a malformed key line can't leak a secret.
    Parse { line: usize, message: String },
    /// A required key is absent.
    MissingKey {
        key: &'static str,
        help: &'static str,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse { line, message } => write!(f, "line {line}: {}", message.trim_end()),
            Self::MissingKey { key, help } => write!(f, "missing required key `{key}`: {help}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTicketmasterKey => write!(
                f,
                "no Ticketmaster API key; venues ticketed through Ticketmaster won't be covered \
                 (set `providers.ticketmaster_key` or {TICKETMASTER_KEY_ENV})"
            ),
        }
    }
}

impl Config {
    /// Parses `config.toml` text, then applies environment overrides.
    ///
    /// `env` looks up an environment variable; pass
    /// `|name| std::env::var(name).ok()` outside of tests. An empty
    /// variable counts as unset. Overrides: [`TICKETMASTER_KEY_ENV`],
    /// [`GOOGLE_CLIENT_ID_ENV`] and [`GOOGLE_CLIENT_SECRET_ENV`].
    ///
    /// # Errors
    ///
    /// [`ConfigError::Parse`] for invalid TOML, wrong value types or
    /// unknown keys; [`ConfigError::MissingKey`] when home is not set, or
    /// only one of the Google client ID and secret is.
    pub fn parse(text: &str, env: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let raw: RawConfig = toml::from_str(text).map_err(|err| ConfigError::Parse {
            line: err
                .span()
                .map_or(1, |span| text[..span.start].matches('\n').count() + 1),
            message: err.message().to_owned(),
        })?;

        let non_empty = |value: &String| !value.is_empty();
        let ticketmaster = env(TICKETMASTER_KEY_ENV)
            .filter(non_empty)
            .or(raw.providers.ticketmaster_key.filter(non_empty));
        let google = google_client(
            env(GOOGLE_CLIENT_ID_ENV)
                .filter(non_empty)
                .or(raw.google.client_id.filter(non_empty)),
            env(GOOGLE_CLIENT_SECRET_ENV)
                .filter(non_empty)
                .or(raw.google.client_secret.filter(non_empty)),
        )?;

        Ok(Self {
            providers: ProviderKeys {
                ticketmaster: ticketmaster.map(Secret::new),
            },
            home: Home {
                place: home_place(raw.home.address, raw.home.lat, raw.home.lon)?,
                radius_miles: raw.home.radius_miles.unwrap_or(DEFAULT_RADIUS_MILES),
            },
            calendar_id: raw
                .calendar
                .calendar_id
                .unwrap_or_else(|| DEFAULT_CALENDAR_ID.to_owned()),
            google,
        })
    }

    /// Non-fatal problems with this config, for the user to see at startup.
    pub fn warnings(&self) -> Vec<Warning> {
        let mut warnings = Vec::new();
        if self.providers.ticketmaster.is_none() {
            warnings.push(Warning::MissingTicketmasterKey);
        }
        warnings
    }
}

fn home_place(
    address: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
) -> Result<HomePlace, ConfigError> {
    const HELP: &str =
        "set `home.address` (address, city or ZIP) or both `home.lat` and `home.lon`";
    match (lat, lon, address) {
        (Some(lat), Some(lon), _) => Ok(HomePlace::Coordinates { lat, lon }),
        (Some(_), None, _) => Err(ConfigError::MissingKey {
            key: "home.lon",
            help: HELP,
        }),
        (None, Some(_), _) => Err(ConfigError::MissingKey {
            key: "home.lat",
            help: HELP,
        }),
        (None, None, Some(address)) => Ok(HomePlace::Address(address)),
        (None, None, None) => Err(ConfigError::MissingKey {
            key: "home.address",
            help: HELP,
        }),
    }
}

fn google_client(
    client_id: Option<String>,
    client_secret: Option<String>,
) -> Result<Option<GoogleClient>, ConfigError> {
    const HELP: &str = "set both `google.client_id` and `google.client_secret` \
         from your OAuth client (see the README), or neither";
    match (client_id, client_secret) {
        (Some(client_id), Some(client_secret)) => Ok(Some(GoogleClient {
            client_id,
            client_secret: Secret::new(client_secret),
        })),
        (Some(_), None) => Err(ConfigError::MissingKey {
            key: "google.client_secret",
            help: HELP,
        }),
        (None, Some(_)) => Err(ConfigError::MissingKey {
            key: "google.client_id",
            help: HELP,
        }),
        (None, None) => Ok(None),
    }
}

// On-disk shape of `config.toml`. Deliberately not `Debug`: it holds raw
// secrets before they are wrapped in `Secret`.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawConfig {
    providers: RawProviders,
    home: RawHome,
    calendar: RawCalendar,
    google: RawGoogle,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawProviders {
    ticketmaster_key: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawHome {
    address: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    radius_miles: Option<f64>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawCalendar {
    calendar_id: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawGoogle {
    client_id: Option<String>,
    client_secret: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    const MINIMAL: &str = r#"
        [home]
        address = "Fort Lauderdale, FL"
    "#;

    #[test]
    fn parses_a_full_config() {
        let config = Config::parse(
            r#"
            [providers]
            ticketmaster_key = "tm-abc123"

            [home]
            lat = 26.1224
            lon = -80.1373
            radius_miles = 40

            [calendar]
            calendar_id = "concerts@group.calendar.google.com"

            [google]
            client_id = "123-abc.apps.googleusercontent.com"
            client_secret = "GOCSPX-test"
            "#,
            no_env,
        )
        .unwrap();

        assert_eq!(
            config,
            Config {
                providers: ProviderKeys {
                    ticketmaster: Some(Secret::new("tm-abc123")),
                },
                home: Home {
                    place: HomePlace::Coordinates {
                        lat: 26.1224,
                        lon: -80.1373,
                    },
                    radius_miles: 40.0,
                },
                calendar_id: "concerts@group.calendar.google.com".into(),
                google: Some(GoogleClient {
                    client_id: "123-abc.apps.googleusercontent.com".into(),
                    client_secret: Secret::new("GOCSPX-test"),
                }),
            }
        );
    }

    #[test]
    fn minimal_config_uses_defaults() {
        let config = Config::parse(MINIMAL, no_env).unwrap();

        assert_eq!(config.providers, ProviderKeys::default());
        assert_eq!(
            config.home.place,
            HomePlace::Address("Fort Lauderdale, FL".into())
        );
        assert_eq!(config.home.radius_miles, DEFAULT_RADIUS_MILES);
        assert_eq!(config.calendar_id, DEFAULT_CALENDAR_ID);
    }

    #[test]
    fn coordinates_win_over_address() {
        let config = Config::parse(
            r#"
            [home]
            address = "Fort Lauderdale, FL"
            lat = 26.1224
            lon = -80.1373
            "#,
            no_env,
        )
        .unwrap();

        assert!(matches!(config.home.place, HomePlace::Coordinates { .. }));
    }

    #[test]
    fn missing_home_is_a_clear_error() {
        let err = Config::parse("", no_env).unwrap_err();

        assert!(matches!(
            err,
            ConfigError::MissingKey {
                key: "home.address",
                ..
            }
        ));
        let shown = err.to_string();
        assert!(shown.contains("home.address"), "{shown}");
        assert!(shown.contains("home.lat"), "{shown}");
    }

    #[test]
    fn lat_without_lon_is_a_missing_key() {
        let err = Config::parse("[home]\nlat = 26.1", no_env).unwrap_err();

        assert!(matches!(
            err,
            ConfigError::MissingKey {
                key: "home.lon",
                ..
            }
        ));
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let err = Config::parse("[home]\nadress = \"typo\"", no_env).unwrap_err();

        assert!(matches!(err, ConfigError::Parse { line: 2, .. }), "{err:?}");
    }

    #[test]
    fn parse_errors_do_not_echo_secrets() {
        // Unquoted value: invalid TOML on the line holding the key.
        let err = Config::parse("[providers]\nticketmaster_key = tm-abc123\n", no_env).unwrap_err();

        let shown = format!("{err} {err:?}");
        assert!(!shown.contains("tm-abc123"), "leaked: {shown}");
        assert!(shown.contains("line 2"), "{shown}");
    }

    fn env_with_key(name: &str) -> Option<String> {
        (name == TICKETMASTER_KEY_ENV).then(|| "tm-from-env".to_owned())
    }

    #[test]
    fn env_var_overrides_the_file_key() {
        let config = Config::parse(
            "[providers]\nticketmaster_key = \"tm-from-file\"\n[home]\naddress = \"33301\"",
            env_with_key,
        )
        .unwrap();

        assert_eq!(
            config.providers.ticketmaster,
            Some(Secret::new("tm-from-env"))
        );
    }

    #[test]
    fn env_var_supplies_a_key_the_file_lacks() {
        let config = Config::parse(MINIMAL, env_with_key).unwrap();

        assert_eq!(
            config.providers.ticketmaster,
            Some(Secret::new("tm-from-env"))
        );
    }

    #[test]
    fn empty_env_var_does_not_override() {
        let config = Config::parse(
            "[providers]\nticketmaster_key = \"tm-from-file\"\n[home]\naddress = \"33301\"",
            |_| Some(String::new()),
        )
        .unwrap();

        assert_eq!(
            config.providers.ticketmaster,
            Some(Secret::new("tm-from-file"))
        );
    }

    #[test]
    fn missing_ticketmaster_key_warns_instead_of_failing() {
        let config = Config::parse(MINIMAL, no_env).unwrap();

        assert_eq!(config.warnings(), [Warning::MissingTicketmasterKey]);
        let shown = Warning::MissingTicketmasterKey.to_string();
        assert!(shown.contains("Ticketmaster"), "{shown}");
        assert!(shown.contains(TICKETMASTER_KEY_ENV), "{shown}");
    }

    #[test]
    fn configured_ticketmaster_key_does_not_warn() {
        let config = Config::parse(MINIMAL, env_with_key).unwrap();

        assert!(config.warnings().is_empty());
    }

    fn home(radius_miles: f64) -> Home {
        Home {
            place: HomePlace::Address("Fort Lauderdale, FL".into()),
            radius_miles,
        }
    }

    #[test]
    fn radius_km_converts_and_rounds_miles() {
        assert_eq!(home(25.0).radius_km(), 40);
        assert_eq!(home(0.4).radius_km(), 1);
    }

    #[test]
    fn negative_radius_km_is_zero() {
        assert_eq!(home(-5.0).radius_km(), 0);
    }

    #[test]
    fn google_client_is_optional() {
        assert_eq!(Config::parse(MINIMAL, no_env).unwrap().google, None);
    }

    #[test]
    fn google_client_comes_from_env_vars() {
        let config = Config::parse(MINIMAL, |name| match name {
            GOOGLE_CLIENT_ID_ENV => Some("env-id".into()),
            GOOGLE_CLIENT_SECRET_ENV => Some("env-secret".into()),
            _ => None,
        })
        .unwrap();

        assert_eq!(
            config.google,
            Some(GoogleClient {
                client_id: "env-id".into(),
                client_secret: Secret::new("env-secret"),
            })
        );
    }

    #[test]
    fn google_env_vars_override_the_file_one_at_a_time() {
        let config = Config::parse(
            "[home]\naddress = \"33301\"\n[google]\nclient_id = \"file-id\"\nclient_secret = \"file-secret\"",
            |name| (name == GOOGLE_CLIENT_SECRET_ENV).then(|| "env-secret".into()),
        )
        .unwrap();

        assert_eq!(
            config.google,
            Some(GoogleClient {
                client_id: "file-id".into(),
                client_secret: Secret::new("env-secret"),
            })
        );
    }

    #[test]
    fn google_client_id_without_secret_is_a_missing_key() {
        let err = Config::parse(
            "[home]\naddress = \"33301\"\n[google]\nclient_id = \"file-id\"",
            no_env,
        )
        .unwrap_err();

        assert!(
            matches!(
                err,
                ConfigError::MissingKey {
                    key: "google.client_secret",
                    ..
                }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn google_client_secret_without_id_is_a_missing_key() {
        let err = Config::parse(
            "[home]\naddress = \"33301\"\n[google]\nclient_secret = \"file-secret\"",
            no_env,
        )
        .unwrap_err();

        assert!(
            matches!(
                err,
                ConfigError::MissingKey {
                    key: "google.client_id",
                    ..
                }
            ),
            "{err:?}"
        );
    }
}
