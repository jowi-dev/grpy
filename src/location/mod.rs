//! Turning what the user gives us into a [`Location`] to search around.
//!
//! The user can name a location in several places. [`LocationResolver`]
//! picks one, in this order:
//!
//! 1. Command-line flags: `--lat/--lon`, or `--near "<city or zip>"`.
//! 2. The home location from the config file.
//!
//! Place names are turned into coordinates by a [`Geocoder`], so tests can
//! swap in a fake one and never touch the network.

mod nominatim;

use std::fmt;

use crate::domain::Location;

pub use nominatim::Nominatim;

/// A location as the user wrote it, before geocoding.
#[derive(Debug, Clone, PartialEq)]
pub enum LocationQuery {
    /// Explicit coordinates in decimal degrees.
    Coordinates {
        /// Latitude, -90 to 90.
        lat: f64,
        /// Longitude, -180 to 180.
        lon: f64,
    },
    /// A free-form place such as `"Pompano Beach, FL"` or `"33060"`.
    Place(String),
}

/// Where a resolved location came from, so the UI can tell the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationSource {
    /// `--lat/--lon` or `--near` on the command line.
    Flags,
    /// The home location in the config file.
    Home,
}

impl fmt::Display for LocationSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LocationSource::Flags => "command line",
            LocationSource::Home => "config home",
        })
    }
}

/// A [`Location`] plus where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLocation {
    /// The location to search around.
    pub location: Location,
    /// Which input it was taken from.
    pub source: LocationSource,
}

impl fmt::Display for ResolvedLocation {
    /// Renders as `"Pompano Beach, FL (26.2379, -80.1248) from command line"`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Location { lat, lon, label } = &self.location;
        write!(f, "{label} ({lat:.4}, {lon:.4}) from {}", self.source)
    }
}

/// Looks up coordinates for a place name.
pub trait Geocoder {
    /// Returns the best match for `place`.
    ///
    /// # Errors
    ///
    /// [`GeocodeError::NotFound`] when nothing matches, or
    /// [`GeocodeError::Lookup`] when the lookup itself fails.
    fn geocode(&self, place: &str) -> Result<Location, GeocodeError>;
}

/// Why a [`Geocoder`] could not find a place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeocodeError {
    /// The geocoder ran but had no match for the place.
    NotFound(String),
    /// The lookup failed (network, bad response, ...).
    Lookup(String),
}

impl fmt::Display for GeocodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GeocodeError::NotFound(place) => write!(f, "no location found for {place:?}"),
            GeocodeError::Lookup(reason) => write!(f, "geocoding failed: {reason}"),
        }
    }
}

impl std::error::Error for GeocodeError {}

/// Why no location could be resolved.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveError {
    /// Neither flags nor the config gave a location.
    NoLocation,
    /// Coordinates were outside the valid latitude/longitude range.
    InvalidCoordinates {
        /// The latitude given.
        lat: f64,
        /// The longitude given.
        lon: f64,
    },
    /// A place name could not be geocoded.
    Geocode(GeocodeError),
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::NoLocation => f.write_str(
                "no location given; pass --near \"<city or zip>\" or --lat/--lon, \
                 or set a home location in the config file",
            ),
            ResolveError::InvalidCoordinates { lat, lon } => write!(
                f,
                "invalid coordinates ({lat}, {lon}); latitude must be -90..=90 \
                 and longitude -180..=180"
            ),
            ResolveError::Geocode(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for ResolveError {}

/// Picks the location to search around from flags and config.
pub struct LocationResolver<G> {
    geocoder: G,
}

impl<G: Geocoder> LocationResolver<G> {
    /// Creates a resolver that geocodes place names with `geocoder`.
    pub fn new(geocoder: G) -> Self {
        Self { geocoder }
    }

    /// Resolves the location to use: `flags` if given, else `home`.
    ///
    /// Only the chosen query is geocoded, so a home location is never
    /// looked up when flags are given.
    ///
    /// # Errors
    ///
    /// [`ResolveError::NoLocation`] when both are `None`,
    /// [`ResolveError::InvalidCoordinates`] for out-of-range coordinates,
    /// and [`ResolveError::Geocode`] when the place name can't be found.
    pub fn resolve(
        &self,
        flags: Option<&LocationQuery>,
        home: Option<&LocationQuery>,
    ) -> Result<ResolvedLocation, ResolveError> {
        let (query, source) = match (flags, home) {
            (Some(query), _) => (query, LocationSource::Flags),
            (None, Some(query)) => (query, LocationSource::Home),
            (None, None) => return Err(ResolveError::NoLocation),
        };
        let location = match query {
            &LocationQuery::Coordinates { lat, lon } => coordinates(lat, lon)?,
            LocationQuery::Place(place) => self
                .geocoder
                .geocode(place)
                .map_err(ResolveError::Geocode)?,
        };
        Ok(ResolvedLocation { location, source })
    }
}

/// Validates raw coordinates and labels them with themselves.
fn coordinates(lat: f64, lon: f64) -> Result<Location, ResolveError> {
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return Err(ResolveError::InvalidCoordinates { lat, lon });
    }
    Ok(Location {
        lat,
        lon,
        label: format!("{lat}, {lon}"),
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// Knows a fixed set of places and records every lookup.
    #[derive(Default)]
    struct FakeGeocoder {
        lookups: RefCell<Vec<String>>,
    }

    impl Geocoder for &FakeGeocoder {
        fn geocode(&self, place: &str) -> Result<Location, GeocodeError> {
            self.lookups.borrow_mut().push(place.to_owned());
            match place {
                "Pompano Beach, FL" => Ok(Location {
                    lat: 26.2379,
                    lon: -80.1248,
                    label: "Pompano Beach, Florida".into(),
                }),
                "33312" => Ok(Location {
                    lat: 26.0897,
                    lon: -80.1789,
                    label: "Fort Lauderdale, Florida 33312".into(),
                }),
                "offline" => Err(GeocodeError::Lookup("connection refused".into())),
                _ => Err(GeocodeError::NotFound(place.to_owned())),
            }
        }
    }

    fn place(name: &str) -> LocationQuery {
        LocationQuery::Place(name.into())
    }

    fn coords(lat: f64, lon: f64) -> LocationQuery {
        LocationQuery::Coordinates { lat, lon }
    }

    #[test]
    fn flag_coordinates_are_used_as_is_without_geocoding() {
        let geocoder = FakeGeocoder::default();
        let resolver = LocationResolver::new(&geocoder);

        let resolved = resolver
            .resolve(Some(&coords(26.1194, -80.1462)), None)
            .unwrap();

        assert_eq!(resolved.source, LocationSource::Flags);
        assert_eq!(resolved.location.lat, 26.1194);
        assert_eq!(resolved.location.lon, -80.1462);
        assert_eq!(resolved.location.label, "26.1194, -80.1462");
        assert!(geocoder.lookups.borrow().is_empty());
    }

    #[test]
    fn flag_place_is_geocoded() {
        let geocoder = FakeGeocoder::default();
        let resolver = LocationResolver::new(&geocoder);

        let resolved = resolver.resolve(Some(&place("33312")), None).unwrap();

        assert_eq!(resolved.source, LocationSource::Flags);
        assert_eq!(resolved.location.label, "Fort Lauderdale, Florida 33312");
        assert_eq!(*geocoder.lookups.borrow(), ["33312"]);
    }

    #[test]
    fn flags_win_over_home_and_home_is_not_looked_up() {
        let geocoder = FakeGeocoder::default();
        let resolver = LocationResolver::new(&geocoder);

        let resolved = resolver
            .resolve(Some(&place("33312")), Some(&place("Pompano Beach, FL")))
            .unwrap();

        assert_eq!(resolved.source, LocationSource::Flags);
        assert_eq!(resolved.location.label, "Fort Lauderdale, Florida 33312");
        assert_eq!(*geocoder.lookups.borrow(), ["33312"]);
    }

    #[test]
    fn home_is_used_when_no_flags_are_given() {
        let geocoder = FakeGeocoder::default();
        let resolver = LocationResolver::new(&geocoder);

        let resolved = resolver
            .resolve(None, Some(&place("Pompano Beach, FL")))
            .unwrap();

        assert_eq!(resolved.source, LocationSource::Home);
        assert_eq!(resolved.location.label, "Pompano Beach, Florida");
    }

    #[test]
    fn home_coordinates_are_used_as_is() {
        let geocoder = FakeGeocoder::default();
        let resolver = LocationResolver::new(&geocoder);

        let resolved = resolver
            .resolve(None, Some(&coords(26.2379, -80.1248)))
            .unwrap();

        assert_eq!(resolved.source, LocationSource::Home);
        assert_eq!(resolved.location.label, "26.2379, -80.1248");
        assert!(geocoder.lookups.borrow().is_empty());
    }

    #[test]
    fn nothing_given_is_an_error() {
        let geocoder = FakeGeocoder::default();
        let resolver = LocationResolver::new(&geocoder);

        assert_eq!(resolver.resolve(None, None), Err(ResolveError::NoLocation));
    }

    #[test]
    fn out_of_range_coordinates_are_rejected() {
        let geocoder = FakeGeocoder::default();
        let resolver = LocationResolver::new(&geocoder);

        for (lat, lon) in [(91.0, 0.0), (-90.5, 0.0), (0.0, 180.5), (0.0, -181.0)] {
            assert_eq!(
                resolver.resolve(Some(&coords(lat, lon)), None),
                Err(ResolveError::InvalidCoordinates { lat, lon }),
            );
        }
    }

    #[test]
    fn non_finite_coordinates_are_rejected() {
        let geocoder = FakeGeocoder::default();
        let resolver = LocationResolver::new(&geocoder);

        let result = resolver.resolve(Some(&coords(f64::NAN, 0.0)), None);

        assert!(matches!(
            result,
            Err(ResolveError::InvalidCoordinates { .. })
        ));
    }

    #[test]
    fn unknown_place_is_a_geocode_error() {
        let geocoder = FakeGeocoder::default();
        let resolver = LocationResolver::new(&geocoder);

        assert_eq!(
            resolver.resolve(Some(&place("Atlantis")), None),
            Err(ResolveError::Geocode(GeocodeError::NotFound(
                "Atlantis".into()
            ))),
        );
    }

    #[test]
    fn failing_lookup_does_not_fall_back_to_home() {
        // A typo in --near should be reported, not silently replaced by home.
        let geocoder = FakeGeocoder::default();
        let resolver = LocationResolver::new(&geocoder);

        let result = resolver.resolve(Some(&place("offline")), Some(&place("Pompano Beach, FL")));

        assert_eq!(
            result,
            Err(ResolveError::Geocode(GeocodeError::Lookup(
                "connection refused".into()
            ))),
        );
    }

    #[test]
    fn resolved_location_shows_label_coordinates_and_source() {
        let resolved = ResolvedLocation {
            location: Location {
                lat: 26.2379,
                lon: -80.1248,
                label: "Pompano Beach, Florida".into(),
            },
            source: LocationSource::Home,
        };

        assert_eq!(
            resolved.to_string(),
            "Pompano Beach, Florida (26.2379, -80.1248) from config home"
        );
    }
}
