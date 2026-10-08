//! Linking a followed venue to its Ticketmaster venue.
//!
//! A venue found on the map or added by hand has no Ticketmaster ID. To
//! ask Ticketmaster for its shows, grpy needs one: either set by hand, or
//! found by matching the venue's name and position against
//! [`venues_near`](crate::provider::EventProvider::venues_near) results.

use crate::domain::{Venue, VenueId};

/// How far apart, in kilometres, a followed venue and a Ticketmaster
/// venue may be and still match by name. Map pins and Ticketmaster's
/// coordinates for the same building can differ by a few hundred metres.
pub const MATCH_RADIUS_KM: f64 = 1.5;

/// The Ticketmaster venue ID for `followed`.
///
/// A `manual` ID always wins. Otherwise picks the `candidates` venue
/// within [`MATCH_RADIUS_KM`] whose name matches: an exact name match
/// beats one name containing the other ("Hard Rock Live" in "Hard Rock
/// Live at Seminole Hard Rock"), and ties go to the nearest. Names are
/// compared ignoring case, punctuation and the words "the" and "and".
/// Returns `None` when nothing matches.
pub fn link_venue(
    followed: &Venue,
    manual: Option<&VenueId>,
    candidates: &[Venue],
) -> Option<VenueId> {
    if let Some(manual) = manual {
        return Some(manual.clone());
    }
    let wanted = name_words(&followed.name);
    candidates
        .iter()
        .filter_map(|candidate| {
            let km = followed.location.distance_km(&candidate.location);
            let quality = name_match(&wanted, &name_words(&candidate.name))?;
            (km <= MATCH_RADIUS_KM).then_some((quality, km, candidate))
        })
        .min_by(|(qa, ka, _), (qb, kb, _)| qa.cmp(qb).then(ka.total_cmp(kb)))
        .map(|(_, _, candidate)| candidate.id.clone())
}

/// How well two names match; lower is better.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum NameMatch {
    Exact,
    Contained,
}

/// Compares two names already split by [`name_words`].
fn name_match(a: &[String], b: &[String]) -> Option<NameMatch> {
    if a.is_empty() || b.is_empty() {
        return None;
    }
    if a == b {
        return Some(NameMatch::Exact);
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short
        .iter()
        .all(|word| long.contains(word))
        .then_some(NameMatch::Contained)
}

/// Lowercase words of `name`, without punctuation or filler words.
fn name_words(name: &str) -> Vec<String> {
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty() && !matches!(*word, "the" | "and"))
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::domain::Location;

    use super::*;

    fn venue(id: &str, name: &str, lat: f64, lon: f64) -> Venue {
        Venue {
            id: id.parse().unwrap(),
            name: name.into(),
            address: None,
            location: Location {
                lat,
                lon,
                label: String::new(),
            },
        }
    }

    /// Hard Rock Live as a map search would report it.
    fn followed() -> Venue {
        venue("osm:node/1", "Hard Rock Live", 26.0510, -80.2105)
    }

    fn id(s: &str) -> VenueId {
        s.parse().unwrap()
    }

    #[test]
    fn same_name_nearby_matches() {
        let candidates = [venue("ticketmaster:A", "Hard Rock Live", 26.0525, -80.2100)];

        assert_eq!(
            link_venue(&followed(), None, &candidates),
            Some(id("ticketmaster:A"))
        );
    }

    #[test]
    fn longer_ticketmaster_name_matches() {
        let candidates = [venue(
            "ticketmaster:A",
            "Hard Rock Live at Seminole Hard Rock Hotel & Casino",
            26.0510,
            -80.2105,
        )];

        assert_eq!(
            link_venue(&followed(), None, &candidates),
            Some(id("ticketmaster:A"))
        );
    }

    #[test]
    fn case_punctuation_and_articles_are_ignored() {
        let followed = venue("osm:node/2", "The Fillmore Miami Beach", 25.7928, -80.1320);
        let candidates = [venue(
            "ticketmaster:F",
            "Fillmore Miami Beach at the Jackie Gleason Theater",
            25.7930,
            -80.1318,
        )];

        assert_eq!(
            link_venue(&followed, None, &candidates),
            Some(id("ticketmaster:F"))
        );
    }

    #[test]
    fn same_name_too_far_away_does_not_match() {
        // Hard Rock Live Orlando, a few hundred km north.
        let candidates = [venue("ticketmaster:O", "Hard Rock Live", 28.4719, -81.4672)];

        assert_eq!(link_venue(&followed(), None, &candidates), None);
    }

    #[test]
    fn different_name_nearby_does_not_match() {
        let candidates = [venue(
            "ticketmaster:G",
            "Guitar Hotel Pool",
            26.0511,
            -80.2104,
        )];

        assert_eq!(link_venue(&followed(), None, &candidates), None);
    }

    #[test]
    fn exact_name_beats_a_partial_match() {
        let candidates = [
            venue(
                "ticketmaster:P",
                "Hard Rock Live Lobby Bar",
                26.0510,
                -80.2105,
            ),
            venue("ticketmaster:E", "Hard Rock Live", 26.0560, -80.2105),
        ];

        assert_eq!(
            link_venue(&followed(), None, &candidates),
            Some(id("ticketmaster:E"))
        );
    }

    #[test]
    fn nearest_wins_between_equal_matches() {
        let candidates = [
            venue("ticketmaster:FAR", "Hard Rock Live", 26.0600, -80.2105),
            venue("ticketmaster:NEAR", "Hard Rock Live", 26.0512, -80.2105),
        ];

        assert_eq!(
            link_venue(&followed(), None, &candidates),
            Some(id("ticketmaster:NEAR"))
        );
    }

    #[test]
    fn manual_id_overrides_matching() {
        let manual = id("ticketmaster:MANUAL");
        let candidates = [venue("ticketmaster:A", "Hard Rock Live", 26.0510, -80.2105)];

        assert_eq!(
            link_venue(&followed(), Some(&manual), &candidates),
            Some(manual.clone())
        );
        assert_eq!(link_venue(&followed(), Some(&manual), &[]), Some(manual));
    }
}
