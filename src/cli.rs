//! Command-line arguments.

use clap::{Parser, Subcommand};

use crate::location::LocationQuery;

/// Find concerts at venues near you and add them to Google Calendar.
#[derive(Debug, Parser)]
#[command(version, about, args_conflicts_with_subcommands = true)]
pub struct Cli {
    /// What to do instead of finding concerts.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Search near this place (city, address or zip code).
    #[arg(long, value_name = "PLACE", conflicts_with_all = ["lat", "lon"])]
    pub near: Option<String>,

    /// Latitude to search around, in decimal degrees. Requires --lon.
    #[arg(long, requires = "lon", allow_negative_numbers = true)]
    pub lat: Option<f64>,

    /// Longitude to search around, in decimal degrees. Requires --lat.
    #[arg(long, requires = "lat", allow_negative_numbers = true)]
    pub lon: Option<f64>,
}

/// grpy's subcommands.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// Sign in to a service grpy uses.
    Auth {
        /// The service to sign in to.
        #[command(subcommand)]
        service: AuthService,
    },
}

/// Services `grpy auth` can sign in to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
pub enum AuthService {
    /// Sign in to Google Calendar in the browser and save the refresh
    /// token, so grpy can add events.
    Google,
}

impl Cli {
    /// The location named by `--near` or `--lat/--lon`, if any.
    pub fn location(&self) -> Option<LocationQuery> {
        match (&self.near, self.lat, self.lon) {
            (Some(place), _, _) => Some(LocationQuery::Place(place.clone())),
            (None, Some(lat), Some(lon)) => Some(LocationQuery::Coordinates { lat, lon }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("grpy").chain(args.iter().copied()))
    }

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_flags_means_no_location() {
        assert_eq!(parse(&[]).unwrap().location(), None);
    }

    #[test]
    fn near_is_a_place_query() {
        assert_eq!(
            parse(&["--near", "Pompano Beach, FL"]).unwrap().location(),
            Some(LocationQuery::Place("Pompano Beach, FL".into()))
        );
    }

    #[test]
    fn lat_and_lon_are_a_coordinate_query() {
        assert_eq!(
            parse(&["--lat", "26.1194", "--lon", "-80.1462"])
                .unwrap()
                .location(),
            Some(LocationQuery::Coordinates {
                lat: 26.1194,
                lon: -80.1462
            })
        );
    }

    #[test]
    fn lat_without_lon_is_rejected() {
        assert!(parse(&["--lat", "26.1194"]).is_err());
    }

    #[test]
    fn near_with_coordinates_is_rejected() {
        assert!(parse(&["--near", "33312", "--lat", "26.1", "--lon", "-80.1"]).is_err());
    }

    #[test]
    fn no_subcommand_runs_the_finder() {
        assert_eq!(parse(&[]).unwrap().command, None);
    }

    #[test]
    fn auth_google_is_a_subcommand() {
        assert_eq!(
            parse(&["auth", "google"]).unwrap().command,
            Some(Command::Auth {
                service: AuthService::Google
            })
        );
    }

    #[test]
    fn auth_needs_a_service() {
        assert!(parse(&["auth"]).is_err());
    }

    #[test]
    fn location_flags_do_not_mix_with_subcommands() {
        assert!(parse(&["--near", "33312", "auth", "google"]).is_err());
    }
}
