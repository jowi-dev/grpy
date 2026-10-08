//! Access tokens on demand, refreshed automatically.

use std::time::{Duration, Instant};

use super::AuthError;
use super::google::{self, TokenEndpoint};
use super::store::TokenStore;
use crate::config::{GoogleClient, Secret};

/// How long before expiry an access token is replaced, so it doesn't
/// expire mid-request.
pub const EXPIRY_MARGIN: Duration = Duration::from_secs(60);

/// Hands out access tokens for the Calendar API, using the stored refresh
/// token to get a new one whenever the current one is about to expire.
pub struct GoogleAuth<E, S> {
    client: GoogleClient,
    endpoint: E,
    store: S,
    cached: Option<(Secret, Instant)>,
}

impl<E: TokenEndpoint, S: TokenStore> GoogleAuth<E, S> {
    /// Signs requests as `client`, with the refresh token in `store`.
    pub fn new(client: GoogleClient, endpoint: E, store: S) -> Self {
        Self {
            client,
            endpoint,
            store,
            cached: None,
        }
    }

    /// An access token valid for at least [`EXPIRY_MARGIN`] after `now`.
    ///
    /// # Errors
    ///
    /// [`AuthError::NotSignedIn`] when no refresh token is stored and
    /// [`AuthError::Revoked`] when Google no longer accepts it; both tell
    /// the user to run `grpy auth google`. Otherwise as
    /// [`google::refresh`].
    pub fn access_token(&mut self, now: Instant) -> Result<Secret, AuthError> {
        if let Some((token, expires_at)) = &self.cached
            && now + EXPIRY_MARGIN < *expires_at
        {
            return Ok(token.clone());
        }
        let refresh_token = self.store.load()?.ok_or(AuthError::NotSignedIn)?;
        let access = google::refresh(&self.endpoint, &self.client, &refresh_token)?;
        self.cached = Some((access.secret.clone(), now + access.expires_in));
        Ok(access.secret)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::auth::google::TokenReply;

    const REFRESH: &str = include_str!("../../tests/fixtures/google/token-refresh.json");
    const INVALID_GRANT: &str = include_str!("../../tests/fixtures/google/invalid-grant.json");

    /// Replies with fixture JSON and counts requests.
    struct FakeEndpoint {
        status: u16,
        body: &'static str,
        calls: Cell<usize>,
    }

    impl FakeEndpoint {
        fn new(status: u16, body: &'static str) -> Self {
            Self {
                status,
                body,
                calls: Cell::new(0),
            }
        }
    }

    impl TokenEndpoint for &FakeEndpoint {
        fn post_form(&self, _form: &[(&str, &str)]) -> Result<TokenReply, String> {
            self.calls.set(self.calls.get() + 1);
            Ok(TokenReply {
                status: self.status,
                body: self.body.to_owned(),
            })
        }
    }

    struct FixedStore(Option<&'static str>);

    impl TokenStore for FixedStore {
        fn load(&self) -> Result<Option<Secret>, AuthError> {
            Ok(self.0.map(Secret::new))
        }

        fn save(&self, _token: &Secret) -> Result<String, AuthError> {
            unreachable!("refreshing never saves")
        }
    }

    fn client() -> GoogleClient {
        GoogleClient {
            client_id: "123-abc.apps.googleusercontent.com".into(),
            client_secret: Secret::new("GOCSPX-test"),
        }
    }

    #[test]
    fn first_call_refreshes() {
        let endpoint = FakeEndpoint::new(200, REFRESH);
        let mut auth = GoogleAuth::new(client(), &endpoint, FixedStore(Some("1//token")));

        let token = auth.access_token(Instant::now()).unwrap();

        assert_eq!(token, Secret::new("ya29.fake-refreshed-token"));
        assert_eq!(endpoint.calls.get(), 1);
    }

    #[test]
    fn token_is_reused_until_close_to_expiry() {
        let endpoint = FakeEndpoint::new(200, REFRESH);
        let mut auth = GoogleAuth::new(client(), &endpoint, FixedStore(Some("1//token")));
        let start = Instant::now();

        auth.access_token(start).unwrap();
        // The fixture token lasts 3599 s.
        auth.access_token(
            start + Duration::from_secs(3599) - EXPIRY_MARGIN - Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(endpoint.calls.get(), 1);

        auth.access_token(start + Duration::from_secs(3599) - EXPIRY_MARGIN)
            .unwrap();
        assert_eq!(endpoint.calls.get(), 2);
    }

    #[test]
    fn no_stored_token_means_not_signed_in() {
        let endpoint = FakeEndpoint::new(200, REFRESH);
        let mut auth = GoogleAuth::new(client(), &endpoint, FixedStore(None));

        let err = auth.access_token(Instant::now()).unwrap_err();

        assert_eq!(err, AuthError::NotSignedIn);
        assert!(err.to_string().contains("grpy auth google"), "{err}");
        assert_eq!(endpoint.calls.get(), 0);
    }

    #[test]
    fn revoked_token_tells_the_user_to_rerun_auth() {
        let endpoint = FakeEndpoint::new(400, INVALID_GRANT);
        let mut auth = GoogleAuth::new(client(), &endpoint, FixedStore(Some("1//revoked")));

        let err = auth.access_token(Instant::now()).unwrap_err();

        assert_eq!(err, AuthError::Revoked);
        assert!(
            err.to_string().contains("re-run `grpy auth google`"),
            "{err}"
        );
    }
}
