//! The HTTP layer under [`Ticketmaster`](super::Ticketmaster), behind a
//! trait so tests can serve fixtures instead of touching the network.

use std::time::Duration;

/// What the client needs from an HTTP response.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Response {
    /// HTTP status code.
    pub status: u16,
    /// The `Retry-After` header in seconds, when present and numeric.
    pub retry_after: Option<u64>,
    /// The response body.
    pub body: String,
}

/// Sends GET requests. Errors are transport failures (DNS, TLS, timeout);
/// HTTP error statuses come back as an `Ok` [`Response`].
pub(super) trait Http: Send + Sync {
    /// GETs `url` with `query` appended as URL-encoded parameters.
    ///
    /// The error message must not contain the URL or query, which carry
    /// the API key.
    fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<Response, String>;
}

/// [`Http`] over a blocking `ureq` agent (rustls, no OpenSSL).
pub(super) struct Ureq(ureq::Agent);

impl Ureq {
    pub(super) fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .user_agent(super::USER_AGENT)
            .build();
        Self(config.into())
    }
}

impl Http for Ureq {
    fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<Response, String> {
        let mut response = self
            .0
            .get(url)
            .query_pairs(query.iter().copied())
            .call()
            .map_err(|err| describe(&err))?;
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse().ok());
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|err| describe(&err))?;
        Ok(Response {
            status: response.status().as_u16(),
            retry_after,
            body,
        })
    }
}

/// Describes a ureq error without the request URL, which holds the key.
fn describe(err: &ureq::Error) -> String {
    match err {
        ureq::Error::Io(io) => io.to_string(),
        ureq::Error::Timeout(_) => "request timed out".into(),
        ureq::Error::HostNotFound => "host not found".into(),
        ureq::Error::ConnectionFailed => "connection failed".into(),
        _ => "request failed".into(),
    }
}
