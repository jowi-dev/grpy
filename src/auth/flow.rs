//! The interactive sign-in behind `grpy auth google`.

use url::Url;

use super::google::{self, TokenEndpoint};
use super::pkce::{Pkce, random_token};
use super::store::TokenStore;
use super::{AuthError, Loopback};
use crate::config::GoogleClient;

/// Runs the installed-app flow and saves the refresh token to `store`.
///
/// Binds a loopback listener, hands Google's consent URL to `show` (which
/// should open it in a browser and print it), waits for the redirect,
/// trades the code for tokens and saves the refresh token. Returns where
/// the token was saved, as [`TokenStore::save`] does.
///
/// # Errors
///
/// [`AuthError::Io`] when the listener can't be bound or no random state
/// can be generated; otherwise as [`Loopback::wait_for_code`],
/// [`google::exchange_code`] and [`TokenStore::save`].
pub fn sign_in(
    client: &GoogleClient,
    endpoint: &impl TokenEndpoint,
    store: &impl TokenStore,
    show: impl FnOnce(&Url),
) -> Result<String, AuthError> {
    let io_error = |err: &dyn std::fmt::Display| AuthError::Io(err.to_string());
    let loopback = Loopback::bind().map_err(|err| io_error(&err))?;
    let redirect_uri = loopback.redirect_uri().map_err(|err| io_error(&err))?;
    let pkce = Pkce::generate().map_err(|err| io_error(&err))?;
    let state = random_token().map_err(|err| io_error(&err))?;

    show(&google::authorize_url(client, &redirect_uri, &pkce, &state));
    let code = loopback.wait_for_code(&state)?;
    let tokens = google::exchange_code(endpoint, client, &code, &pkce, &redirect_uri)?;
    store.save(&tokens.refresh)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::thread;

    use super::*;
    use crate::auth::google::TokenReply;
    use crate::config::Secret;

    const EXCHANGE: &str = include_str!("../../tests/fixtures/google/token-exchange.json");

    struct FakeEndpoint;

    impl TokenEndpoint for FakeEndpoint {
        fn post_form(&self, form: &[(&str, &str)]) -> Result<TokenReply, String> {
            let form: HashMap<_, _> = form.iter().copied().collect();
            assert_eq!(form["code"], "4/0Abc");
            Ok(TokenReply {
                status: 200,
                body: EXCHANGE.to_owned(),
            })
        }
    }

    #[derive(Default)]
    struct MemoryStore(RefCell<Option<Secret>>);

    impl TokenStore for MemoryStore {
        fn load(&self) -> Result<Option<Secret>, AuthError> {
            Ok(self.0.borrow().clone())
        }

        fn save(&self, token: &Secret) -> Result<String, AuthError> {
            *self.0.borrow_mut() = Some(token.clone());
            Ok("memory".into())
        }
    }

    /// Plays the browser: follows Google's redirect to grpy's listener.
    fn redirect_back(consent_url: &Url, code: &str) -> thread::JoinHandle<String> {
        let query: HashMap<_, _> = consent_url.query_pairs().into_owned().collect();
        let redirect = Url::parse(&query["redirect_uri"]).unwrap();
        let state = query["state"].clone();
        let code = code.to_owned();
        thread::spawn(move || {
            let mut stream =
                TcpStream::connect((redirect.host_str().unwrap(), redirect.port().unwrap()))
                    .unwrap();
            write!(
                stream,
                "GET /?state={state}&code={code} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"
            )
            .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        })
    }

    #[test]
    fn sign_in_saves_the_refresh_token_from_the_redirect() {
        let store = MemoryStore::default();
        let mut browser = None;

        let location = sign_in(
            &GoogleClient {
                client_id: "123-abc.apps.googleusercontent.com".into(),
                client_secret: Secret::new("GOCSPX-test"),
            },
            &FakeEndpoint,
            &store,
            |url| browser = Some(redirect_back(url, "4%2F0Abc")),
        )
        .unwrap();

        assert_eq!(location, "memory");
        assert_eq!(
            *store.0.borrow(),
            Some(Secret::new("1//fake-refresh-token"))
        );
        let response = browser.unwrap().join().unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    }
}
