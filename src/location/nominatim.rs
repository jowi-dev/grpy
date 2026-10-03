//! [`Geocoder`] backed by OpenStreetMap's Nominatim search API.
//!
//! Follows the [Nominatim usage policy]: requests carry a User-Agent that
//! identifies grpy, and grpy makes at most one lookup per run.
//!
//! [Nominatim usage policy]: https://operations.osmfoundation.org/policies/nominatim/

use serde::Deserialize;

use super::{GeocodeError, Geocoder};
use crate::domain::Location;

/// The public Nominatim instance.
pub const DEFAULT_BASE_URL: &str = "https://nominatim.openstreetmap.org";

/// User-Agent sent with every request, as the usage policy requires.
const USER_AGENT: &str = concat!(
    "grpy/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/jowi-dev/grpy)"
);

/// Geocodes place names with a Nominatim server.
pub struct Nominatim {
    base_url: String,
    agent: ureq::Agent,
}

impl Nominatim {
    /// A geocoder that talks to the public instance at [`DEFAULT_BASE_URL`].
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_BASE_URL)
    }

    /// A geocoder that talks to a self-hosted Nominatim at `base_url`.
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            agent: ureq::Agent::new_with_defaults(),
        }
    }
}

impl Default for Nominatim {
    fn default() -> Self {
        Self::new()
    }
}

impl Geocoder for Nominatim {
    fn geocode(&self, place: &str) -> Result<Location, GeocodeError> {
        let body = self
            .agent
            .get(format!("{}/search", self.base_url.trim_end_matches('/')))
            .header("User-Agent", USER_AGENT)
            .query("q", place)
            .query("format", "jsonv2")
            .query("limit", "1")
            .call()
            .and_then(|mut response| response.body_mut().read_to_string())
            .map_err(|err| GeocodeError::Lookup(err.to_string()))?;
        parse_search(place, &body)
    }
}

/// One result from `/search?format=jsonv2`. Nominatim sends coordinates
/// as strings.
#[derive(Deserialize)]
struct SearchResult {
    lat: String,
    lon: String,
    display_name: String,
}

/// Takes the first match from a `/search?format=jsonv2` response body.
fn parse_search(place: &str, body: &str) -> Result<Location, GeocodeError> {
    let results: Vec<SearchResult> = serde_json::from_str(body)
        .map_err(|err| GeocodeError::Lookup(format!("unexpected Nominatim response: {err}")))?;
    let first = results
        .into_iter()
        .next()
        .ok_or_else(|| GeocodeError::NotFound(place.to_owned()))?;
    let coordinate = |value: &str| {
        value
            .parse::<f64>()
            .map_err(|_| GeocodeError::Lookup(format!("bad coordinate {value:?} from Nominatim")))
    };
    Ok(Location {
        lat: coordinate(&first.lat)?,
        lon: coordinate(&first.lon)?,
        label: first.display_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const POMPANO_BEACH: &str = include_str!("../../tests/fixtures/nominatim/pompano-beach.json");

    #[test]
    fn first_result_becomes_the_location() {
        let location = parse_search("Pompano Beach, FL", POMPANO_BEACH).unwrap();

        assert_eq!(
            location,
            Location {
                lat: 26.2378597,
                lon: -80.1247667,
                label: "Pompano Beach, Broward County, Florida, United States".into(),
            }
        );
    }

    #[test]
    fn empty_result_list_is_not_found() {
        assert_eq!(
            parse_search("Atlantis", "[]"),
            Err(GeocodeError::NotFound("Atlantis".into()))
        );
    }

    #[test]
    fn malformed_body_is_a_lookup_error() {
        assert!(matches!(
            parse_search("Pompano Beach, FL", "<html>rate limited</html>"),
            Err(GeocodeError::Lookup(_))
        ));
    }

    #[test]
    fn unparseable_coordinates_are_a_lookup_error() {
        let body = r#"[{"lat":"north","lon":"-80.12","display_name":"Somewhere"}]"#;

        assert!(matches!(
            parse_search("Somewhere", body),
            Err(GeocodeError::Lookup(_))
        ));
    }
}
