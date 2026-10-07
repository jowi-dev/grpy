//! Google's OAuth 2.0 endpoints: the consent URL, the code exchange and
//! token refresh.
//!
//! HTTP goes through a [`TokenEndpoint`], so tests can answer with fixture
//! JSON instead of calling Google.

use std::time::Duration;

use serde::Deserialize;
use url::Url;

use super::{AuthError, CALENDAR_EVENTS_SCOPE, Pkce};
use crate::config::{GoogleClient, Secret};

/// Google's consent page.
pub const AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";

/// Google's token endpoint, for the code exchange and refreshes.
pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// An HTTP response from the token endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenReply {
    /// HTTP status code.
    pub status: u16,
    /// Response body, JSON when Google answered.
    pub body: String,
}

/// Sends a form-encoded POST to Google's token endpoint.
pub trait TokenEndpoint {
    /// Posts `form` and returns the response, whatever its status.
    ///
    /// # Errors
    ///
    /// A description of the failure when no response arrived at all.
    fn post_form(&self, form: &[(&str, &str)]) -> Result<TokenReply, String>;
}

/// [`TokenEndpoint`] that calls [`TOKEN_URL`] over HTTPS.
pub struct HttpTokenEndpoint {
    agent: ureq::Agent,
}

impl HttpTokenEndpoint {
    /// An endpoint that calls Google.
    pub fn new() -> Self {
        Self {
            agent: ureq::Agent::config_builder()
                .http_status_as_error(false)
                .build()
                .into(),
        }
    }
}

impl Default for HttpTokenEndpoint {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenEndpoint for HttpTokenEndpoint {
    fn post_form(&self, form: &[(&str, &str)]) -> Result<TokenReply, String> {
        let mut response = self
            .agent
            .post(TOKEN_URL)
            .send_form(form.iter().copied())
            .map_err(|err| err.to_string())?;
        Ok(TokenReply {
            status: response.status().as_u16(),
            body: response
                .body_mut()
                .read_to_string()
                .map_err(|err| err.to_string())?,
        })
    }
}

/// A short-lived token for calling the Calendar API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessToken {
    /// The bearer token.
    pub secret: Secret,
    /// How long Google says it stays valid, from when it was issued.
    pub expires_in: Duration,
}

/// What a successful code exchange returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tokens {
    /// Valid for about an hour.
    pub access: AccessToken,
    /// Long-lived; trades for new access tokens until revoked.
    pub refresh: Secret,
}

/// The URL of Google's consent page for this sign-in.
///
/// Asks only for [`CALENDAR_EVENTS_SCOPE`], with `access_type=offline`
/// and `prompt=consent` so Google always returns a refresh token.
/// `redirect_uri` is grpy's loopback listener and `state` a random value
/// the redirect must echo back.
pub fn authorize_url(client: &GoogleClient, redirect_uri: &str, pkce: &Pkce, state: &str) -> Url {
    let mut url = Url::parse(AUTHORIZE_URL).expect("AUTHORIZE_URL is a valid URL");
    url.query_pairs_mut()
        .append_pair("client_id", &client.client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", CALENDAR_EVENTS_SCOPE)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    url
}

/// Trades the authorization code from the redirect for tokens.
///
/// # Errors
///
/// [`AuthError::MissingScope`] when the user didn't grant calendar
/// access, [`AuthError::NoRefreshToken`] when Google withheld one,
/// [`AuthError::OAuth`] for errors from Google and
/// [`AuthError::Network`] when Google can't be reached or answers with
/// something other than JSON.
pub fn exchange_code(
    endpoint: &impl TokenEndpoint,
    client: &GoogleClient,
    code: &str,
    pkce: &Pkce,
    redirect_uri: &str,
) -> Result<Tokens, AuthError> {
    let reply = post(
        endpoint,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", pkce.verifier.expose()),
            ("redirect_uri", redirect_uri),
            ("client_id", &client.client_id),
            ("client_secret", client.client_secret.expose()),
        ],
    )?;
    let granted = reply.scope.as_deref().unwrap_or_default();
    if !granted.split(' ').any(|scope| scope == CALENDAR_EVENTS_SCOPE) {
        return Err(AuthError::MissingScope);
    }
    let refresh = reply
        .refresh_token
        .clone()
        .ok_or(AuthError::NoRefreshToken)?;
    Ok(Tokens {
        access: reply.access_token(),
        refresh: Secret::new(refresh),
    })
}

/// Gets a fresh access token with a refresh token.
///
/// # Errors
///
/// [`AuthError::Revoked`] when Google no longer accepts the refresh
/// token; otherwise as [`exchange_code`].
pub fn refresh(
    endpoint: &impl TokenEndpoint,
    client: &GoogleClient,
    refresh_token: &Secret,
) -> Result<AccessToken, AuthError> {
    let reply = post(
        endpoint,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.expose()),
            ("client_id", &client.client_id),
            ("client_secret", client.client_secret.expose()),
        ],
    )
    .map_err(|err| match err {
        AuthError::OAuth { error, .. } if error == "invalid_grant" => AuthError::Revoked,
        other => other,
    })?;
    Ok(reply.access_token())
}

// A token endpoint success response. Deliberately not `Debug`: it holds
// raw tokens.
#[derive(Deserialize)]
struct TokenSuccess {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
    scope: Option<String>,
}

impl TokenSuccess {
    fn access_token(self) -> AccessToken {
        AccessToken {
            secret: Secret::new(self.access_token),
            expires_in: Duration::from_secs(self.expires_in),
        }
    }
}

#[derive(Deserialize)]
struct TokenFailure {
    error: String,
    error_description: Option<String>,
}

/// Posts `form` and decodes Google's answer, success or OAuth error.
fn post(endpoint: &impl TokenEndpoint, form: &[(&str, &str)]) -> Result<TokenSuccess, AuthError> {
    let reply = endpoint.post_form(form).map_err(AuthError::Network)?;
    if (200..300).contains(&reply.status) {
        return serde_json::from_str(&reply.body).map_err(|err| {
            AuthError::Network(format!("unexpected token response from Google: {err}"))
        });
    }
    match serde_json::from_str::<TokenFailure>(&reply.body) {
        Ok(failure) => Err(AuthError::OAuth {
            error: failure.error,
            description: failure.error_description,
        }),
        Err(_) => Err(AuthError::Network(format!(
            "Google's token endpoint answered HTTP {}",
            reply.status
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    const EXCHANGE: &str = include_str!("../../tests/fixtures/google/token-exchange.json");
    const REFRESH: &str = include_str!("../../tests/fixtures/google/token-refresh.json");
    const INVALID_GRANT: &str = include_str!("../../tests/fixtures/google/invalid-grant.json");
    const INVALID_CLIENT: &str = include_str!("../../tests/fixtures/google/invalid-client.json");

    const REDIRECT: &str = "http://127.0.0.1:8765/";

    /// Answers every request with one canned reply and records the forms.
    struct FakeEndpoint {
        reply: Result<TokenReply, String>,
        forms: RefCell<Vec<HashMap<String, String>>>,
    }

    impl FakeEndpoint {
        fn replying(status: u16, body: &str) -> Self {
            Self {
                reply: Ok(TokenReply {
                    status,
                    body: body.to_owned(),
                }),
                forms: RefCell::default(),
            }
        }

        fn last_form(&self) -> HashMap<String, String> {
            self.forms.borrow().last().cloned().expect("no request sent")
        }
    }

    impl TokenEndpoint for FakeEndpoint {
        fn post_form(&self, form: &[(&str, &str)]) -> Result<TokenReply, String> {
            self.forms.borrow_mut().push(
                form.iter()
                    .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                    .collect(),
            );
            self.reply.clone()
        }
    }

    fn client() -> GoogleClient {
        GoogleClient {
            client_id: "123-abc.apps.googleusercontent.com".into(),
            client_secret: Secret::new("GOCSPX-test"),
        }
    }

    fn pkce() -> Pkce {
        Pkce::from_verifier("dBjftJeZ4CVP-mJ92K9mfGx1gq74d6XYvUKOh2HlAQs")
    }

    #[test]
    fn authorize_url_requests_offline_calendar_events_access_with_pkce() {
        let url = authorize_url(&client(), REDIRECT, &pkce(), "state-123");

        assert!(url.as_str().starts_with(AUTHORIZE_URL), "{url}");
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        let expected: HashMap<String, String> = [
            ("client_id", "123-abc.apps.googleusercontent.com"),
            ("redirect_uri", REDIRECT),
            ("response_type", "code"),
            ("scope", CALENDAR_EVENTS_SCOPE),
            ("code_challenge", "06aTZDjdj5HRPy5S-SZuIiTIBQqvQXWhUi0w-idGegA"),
            ("code_challenge_method", "S256"),
            ("state", "state-123"),
            ("access_type", "offline"),
            ("prompt", "consent"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();
        assert_eq!(query, expected);
    }

    #[test]
    fn authorize_url_never_contains_the_secret_or_verifier() {
        let url = authorize_url(&client(), REDIRECT, &pkce(), "state-123").to_string();

        assert!(!url.contains("GOCSPX-test"), "{url}");
        assert!(!url.contains(pkce().verifier.expose()), "{url}");
    }

    #[test]
    fn exchange_sends_the_code_and_verifier() {
        let endpoint = FakeEndpoint::replying(200, EXCHANGE);

        exchange_code(&endpoint, &client(), "auth-code", &pkce(), REDIRECT).unwrap();

        let form = endpoint.last_form();
        assert_eq!(form["grant_type"], "authorization_code");
        assert_eq!(form["code"], "auth-code");
        assert_eq!(form["code_verifier"], pkce().verifier.expose());
        assert_eq!(form["redirect_uri"], REDIRECT);
        assert_eq!(form["client_id"], "123-abc.apps.googleusercontent.com");
        assert_eq!(form["client_secret"], "GOCSPX-test");
    }

    #[test]
    fn exchange_returns_access_and_refresh_tokens() {
        let endpoint = FakeEndpoint::replying(200, EXCHANGE);

        let tokens = exchange_code(&endpoint, &client(), "auth-code", &pkce(), REDIRECT).unwrap();

        assert_eq!(
            tokens,
            Tokens {
                access: AccessToken {
                    secret: Secret::new("ya29.fake-access-token"),
                    expires_in: Duration::from_secs(3599),
                },
                refresh: Secret::new("1//fake-refresh-token"),
            }
        );
    }

    #[test]
    fn exchange_without_a_refresh_token_fails() {
        let endpoint = FakeEndpoint::replying(200, REFRESH);

        assert_eq!(
            exchange_code(&endpoint, &client(), "auth-code", &pkce(), REDIRECT),
            Err(AuthError::NoRefreshToken)
        );
    }

    #[test]
    fn exchange_without_the_calendar_scope_fails() {
        let body = EXCHANGE.replace(CALENDAR_EVENTS_SCOPE, "openid");
        let endpoint = FakeEndpoint::replying(200, &body);

        assert_eq!(
            exchange_code(&endpoint, &client(), "auth-code", &pkce(), REDIRECT),
            Err(AuthError::MissingScope)
        );
    }

    #[test]
    fn exchange_accepts_the_scope_among_others() {
        let body = EXCHANGE.replace(
            CALENDAR_EVENTS_SCOPE,
            &format!("openid {CALENDAR_EVENTS_SCOPE} email"),
        );
        let endpoint = FakeEndpoint::replying(200, &body);

        assert!(exchange_code(&endpoint, &client(), "auth-code", &pkce(), REDIRECT).is_ok());
    }

    #[test]
    fn refresh_sends_the_refresh_token() {
        let endpoint = FakeEndpoint::replying(200, REFRESH);

        refresh(&endpoint, &client(), &Secret::new("1//fake-refresh-token")).unwrap();

        let form = endpoint.last_form();
        assert_eq!(form["grant_type"], "refresh_token");
        assert_eq!(form["refresh_token"], "1//fake-refresh-token");
        assert_eq!(form["client_id"], "123-abc.apps.googleusercontent.com");
        assert_eq!(form["client_secret"], "GOCSPX-test");
    }

    #[test]
    fn refresh_returns_a_new_access_token() {
        let endpoint = FakeEndpoint::replying(200, REFRESH);

        assert_eq!(
            refresh(&endpoint, &client(), &Secret::new("1//fake-refresh-token")),
            Ok(AccessToken {
                secret: Secret::new("ya29.fake-refreshed-token"),
                expires_in: Duration::from_secs(3599),
            })
        );
    }

    #[test]
    fn revoked_refresh_token_says_to_rerun_auth() {
        let endpoint = FakeEndpoint::replying(400, INVALID_GRANT);

        let err = refresh(&endpoint, &client(), &Secret::new("1//revoked")).unwrap_err();

        assert_eq!(err, AuthError::Revoked);
        assert!(err.to_string().contains("re-run `grpy auth google`"), "{err}");
    }

    #[test]
    fn other_oauth_errors_keep_googles_explanation() {
        let endpoint = FakeEndpoint::replying(401, INVALID_CLIENT);

        assert_eq!(
            refresh(&endpoint, &client(), &Secret::new("1//fake-refresh-token")),
            Err(AuthError::OAuth {
                error: "invalid_client".into(),
                description: Some("The OAuth client was not found.".into()),
            })
        );
    }

    #[test]
    fn invalid_grant_on_exchange_is_not_a_revocation() {
        let endpoint = FakeEndpoint::replying(400, INVALID_GRANT);

        assert!(matches!(
            exchange_code(&endpoint, &client(), "stale-code", &pkce(), REDIRECT),
            Err(AuthError::OAuth { .. })
        ));
    }

    #[test]
    fn non_json_reply_is_a_network_error() {
        let endpoint = FakeEndpoint::replying(502, "<html>Bad Gateway</html>");

        assert!(matches!(
            refresh(&endpoint, &client(), &Secret::new("1//fake-refresh-token")),
            Err(AuthError::Network(_))
        ));
    }

    #[test]
    fn transport_failure_is_a_network_error() {
        let endpoint = FakeEndpoint {
            reply: Err("connection refused".into()),
            forms: RefCell::default(),
        };

        assert_eq!(
            refresh(&endpoint, &client(), &Secret::new("1//fake-refresh-token")),
            Err(AuthError::Network("connection refused".into()))
        );
    }

    #[test]
    fn tokens_debug_output_is_redacted() {
        let endpoint = FakeEndpoint::replying(200, EXCHANGE);

        let tokens = exchange_code(&endpoint, &client(), "auth-code", &pkce(), REDIRECT).unwrap();
        let shown = format!("{tokens:?}");

        assert!(!shown.contains("fake-access-token"), "{shown}");
        assert!(!shown.contains("fake-refresh-token"), "{shown}");
    }
}
