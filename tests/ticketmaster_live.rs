//! Live smoke test against the real Ticketmaster Discovery API.
//!
//! Ignored by default: it needs network access and an API key, and the
//! Nix sandbox has neither. Run it by hand with
//!
//! ```sh
//! GRPY_TICKETMASTER_KEY=... cargo test --test ticketmaster_live -- --ignored --nocapture
//! ```
//!
//! It doubles as the coverage check for the venues the data-source spike
//! (docs/decisions/0001-data-source.md) expects Ticketmaster to cover, and
//! prints each one's Ticketmaster venue ID.

use grpy::config::{Secret, TICKETMASTER_KEY_ENV};
use grpy::domain::{Location, Venue};
use grpy::provider::ticketmaster::link_venue;
use grpy::provider::{EventProvider, Ticketmaster};

/// Venues the spike found selling through Ticketmaster: name and
/// approximate coordinates, as a map search would report them.
const SPIKE_VENUES: [(&str, f64, f64); 7] = [
    ("Hard Rock Live", 26.0510, -80.2105),
    ("Culture Room", 26.1653, -80.1178),
    ("Revolution Live", 26.1194, -80.1462),
    ("iTHINK Financial Amphitheatre", 26.6911, -80.1744),
    ("The Parker", 26.1318, -80.1335),
    ("Broward Center", 26.1192, -80.1494),
    ("The Fillmore Miami Beach", 25.7926, -80.1323),
];

/// Search radius around each venue. Small, so a dense area can't push the
/// venue past the API's 1000-result paging limit.
const SEARCH_RADIUS_KM: u32 = 5;

#[tokio::test]
#[ignore = "calls the live Ticketmaster API; needs GRPY_TICKETMASTER_KEY"]
async fn spike_venues_are_on_ticketmaster() {
    let key = std::env::var(TICKETMASTER_KEY_ENV)
        .ok()
        .filter(|key| !key.is_empty())
        .unwrap_or_else(|| panic!("set {TICKETMASTER_KEY_ENV} to run this test"));
    let provider = Ticketmaster::new(Secret::new(key));

    let mut missing = Vec::new();
    for (name, lat, lon) in SPIKE_VENUES {
        let followed = Venue {
            id: format!("manual:{name}").parse().unwrap(),
            name: name.into(),
            address: None,
            location: Location {
                lat,
                lon,
                label: name.into(),
            },
        };
        let nearby = provider
            .venues_near(&followed.location, SEARCH_RADIUS_KM)
            .await
            .unwrap_or_else(|err| panic!("searching near {name}: {err}"));

        match link_venue(&followed, None, &nearby) {
            Some(id) => {
                let found = nearby.iter().find(|venue| venue.id == id).unwrap();
                println!("{name}: {id} ({})", found.name);
            }
            None => {
                println!("{name}: NOT FOUND among {} nearby venues:", nearby.len());
                for venue in &nearby {
                    println!(
                        "    {} {:?} {:.2} km",
                        venue.id,
                        venue.name,
                        followed.location.distance_km(&venue.location)
                    );
                }
                missing.push(name);
            }
        }
    }

    assert!(
        missing.is_empty(),
        "not matched on Ticketmaster: {missing:?}"
    );
}
