//! Provider-namespaced identifiers.
//!
//! Every venue and event ID carries the provider it came from, written as
//! `provider:local-id` (for example `ticketmaster:KovZpZAEkvEA`). This lets
//! several providers report the same venue or show without their IDs
//! colliding.

use std::fmt;
use std::str::FromStr;

/// Name of the provider an ID belongs to, such as `ticketmaster` or `ics`.
///
/// Must be non-empty and contain only lowercase ASCII letters, digits and
/// `-`, so it can never contain the `:` separator.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProviderId(String);

/// Error returned when parsing a [`ProviderId`], [`VenueId`] or [`EventId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseIdError {
    /// No `:` separating the provider from the provider-local ID.
    MissingSeparator,
    /// The provider name is empty or contains characters other than
    /// lowercase ASCII letters, digits and `-`.
    InvalidProvider,
    /// The provider-local part after the `:` is empty.
    EmptyLocalId,
}

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MissingSeparator => "expected `provider:id`, found no `:`",
            Self::InvalidProvider => {
                "provider must be non-empty lowercase ASCII letters, digits or `-`"
            }
            Self::EmptyLocalId => "ID after `provider:` is empty",
        })
    }
}

impl std::error::Error for ParseIdError {}

impl FromStr for ProviderId {
    type Err = ParseIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let valid = !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if valid {
            Ok(Self(s.to_owned()))
        } else {
            Err(ParseIdError::InvalidProvider)
        }
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

macro_rules! namespaced_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name {
            provider: ProviderId,
            local: String,
        }

        impl $name {
            /// Builds an ID from a provider and its provider-local ID.
            ///
            /// Returns [`ParseIdError::EmptyLocalId`] if `local` is empty.
            pub fn new(
                provider: ProviderId,
                local: impl Into<String>,
            ) -> Result<Self, ParseIdError> {
                let local = local.into();
                if local.is_empty() {
                    return Err(ParseIdError::EmptyLocalId);
                }
                Ok(Self { provider, local })
            }

            /// The provider this ID belongs to.
            pub fn provider(&self) -> &ProviderId {
                &self.provider
            }

            /// The provider-local part of the ID, after the first `:`.
            pub fn local(&self) -> &str {
                &self.local
            }
        }

        impl FromStr for $name {
            type Err = ParseIdError;

            /// Parses `provider:local-id`, splitting on the first `:`. The
            /// local part may itself contain `:` (for example a URL).
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let (provider, local) =
                    s.split_once(':').ok_or(ParseIdError::MissingSeparator)?;
                Self::new(provider.parse()?, local)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}:{}", self.provider, self.local)
            }
        }
    };
}

namespaced_id!(
    /// Identifier for a [`Venue`](super::Venue), namespaced by provider.
    VenueId
);

namespaced_id!(
    /// Identifier for an [`Event`](super::Event), namespaced by provider.
    EventId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venue_id_round_trips_through_display() {
        let id: VenueId = "ticketmaster:KovZpZAEkvEA".parse().unwrap();
        assert_eq!(id.provider().to_string(), "ticketmaster");
        assert_eq!(id.local(), "KovZpZAEkvEA");
        assert_eq!(id.to_string(), "ticketmaster:KovZpZAEkvEA");
    }

    #[test]
    fn event_id_round_trips_through_display() {
        let id: EventId = "ticketmaster:Z7r9jZ1A7o8eO".parse().unwrap();
        assert_eq!(id.provider().to_string(), "ticketmaster");
        assert_eq!(id.local(), "Z7r9jZ1A7o8eO");
        assert_eq!(id.to_string(), "ticketmaster:Z7r9jZ1A7o8eO");
    }

    #[test]
    fn local_id_may_contain_colons() {
        let id: EventId = "ics:https://example.com/events/1".parse().unwrap();
        assert_eq!(id.provider().to_string(), "ics");
        assert_eq!(id.local(), "https://example.com/events/1");
        assert_eq!(id.to_string(), "ics:https://example.com/events/1");
    }

    #[test]
    fn new_builds_the_same_id_as_parsing() {
        let provider: ProviderId = "json-ld".parse().unwrap();
        let built = VenueId::new(provider, "ithink").unwrap();
        assert_eq!(built, "json-ld:ithink".parse().unwrap());
    }

    #[test]
    fn new_rejects_empty_local_id() {
        let provider: ProviderId = "ticketmaster".parse().unwrap();
        assert_eq!(EventId::new(provider, ""), Err(ParseIdError::EmptyLocalId));
    }

    #[test]
    fn missing_separator_is_rejected() {
        assert_eq!(
            "KovZpZAEkvEA".parse::<VenueId>(),
            Err(ParseIdError::MissingSeparator)
        );
    }

    #[test]
    fn empty_provider_is_rejected() {
        assert_eq!(
            ":KovZpZAEkvEA".parse::<VenueId>(),
            Err(ParseIdError::InvalidProvider)
        );
    }

    #[test]
    fn provider_with_invalid_characters_is_rejected() {
        for bad in ["Ticketmaster", "ticket master", "ticket_master", "tm/1"] {
            assert_eq!(
                bad.parse::<ProviderId>(),
                Err(ParseIdError::InvalidProvider),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn empty_local_id_is_rejected() {
        assert_eq!(
            "ticketmaster:".parse::<EventId>(),
            Err(ParseIdError::EmptyLocalId)
        );
    }

    #[test]
    fn venue_and_event_ids_parse_the_same_way() {
        let venue: VenueId = "ics:revolution-live".parse().unwrap();
        let event: EventId = "ics:revolution-live".parse().unwrap();
        assert_eq!(venue.to_string(), event.to_string());
    }

    #[test]
    fn parse_error_messages_are_descriptive() {
        assert_eq!(
            ParseIdError::MissingSeparator.to_string(),
            "expected `provider:id`, found no `:`"
        );
    }
}
