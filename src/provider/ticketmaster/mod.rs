//! [`EventProvider`] backed by the [Ticketmaster Discovery API].
//!
//! Requests are sent one at a time, at most 5 per second (the API's
//! per-key rate limit). The blocking HTTP calls run on tokio's blocking
//! thread pool, so they never stall the async runtime.
//!
//! [Ticketmaster Discovery API]: https://developer.ticketmaster.com/products-and-docs/apis/discovery-api/v2/

mod api;
mod http;

use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::config::Secret;
use crate::domain::{DateRange, Event, Location, ProviderId, Venue, VenueId};

use super::EventProvider;
use http::{Http, Ureq};

/// The provider name used in Ticketmaster venue and event IDs.
pub const PROVIDER: &str = "ticketmaster";

/// The public Discovery API host.
pub const DEFAULT_BASE_URL: &str = "https://app.ticketmaster.com";

/// Results requested per page; the API's maximum.
const PAGE_SIZE: u32 = 200;

/// The API refuses to page past this many results (`size * page < 1000`).
const MAX_RESULTS: u32 = 1000;

/// User-Agent sent with every request.
const USER_AGENT: &str = concat!(
    "grpy/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/jowi-dev/grpy)"
);

fn provider_id() -> ProviderId {
    PROVIDER.parse().expect("PROVIDER is a valid provider ID")
}

/// Why a Ticketmaster request failed.
///
/// [`EventProvider`] methods wrap this in [`super::Error::Other`]; callers
/// that care can downcast to it. No variant carries the API key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// HTTP 429: the per-second rate limit or the daily quota was hit.
    RateLimited {
        /// Seconds the API asked us to wait, from `Retry-After`.
        retry_after: Option<u64>,
    },
    /// Any other non-success HTTP status.
    Status(u16),
    /// The request never got a response (DNS, TLS, timeout, ...).
    Request(String),
    /// The response body was not what the Discovery API sends.
    Response(String),
    /// An event ID with characters other than ASCII letters, digits, `-`
    /// and `_`, which would not be a single URL path segment.
    InvalidEventId,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RateLimited {
                retry_after: Some(secs),
            } => write!(
                f,
                "Ticketmaster rate limit hit (HTTP 429); try again in {secs}s"
            ),
            Self::RateLimited { retry_after: None } => f.write_str(
                "Ticketmaster rate limit or daily quota hit (HTTP 429); try again later",
            ),
            Self::Status(status) => write!(f, "Ticketmaster returned HTTP {status}"),
            Self::Request(reason) => write!(f, "Ticketmaster request failed: {reason}"),
            Self::Response(reason) => f.write_str(reason),
            Self::InvalidEventId => {
                f.write_str("a Ticketmaster event ID is only letters, digits, `-` and `_`")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<Error> for super::Error {
    fn from(err: Error) -> Self {
        super::Error::Other(Box::new(err))
    }
}

/// Finds venues through the Ticketmaster Discovery API.
///
/// Cheap to clone; clones share one rate limit.
#[derive(Clone)]
pub struct Ticketmaster {
    client: Arc<Client>,
}

impl Ticketmaster {
    /// A provider that calls the public API at [`DEFAULT_BASE_URL`] with
    /// `api_key` (the config's `providers.ticketmaster_key`).
    pub fn new(api_key: Secret) -> Self {
        Self::with_base_url(api_key, DEFAULT_BASE_URL)
    }

    /// A provider that calls the Discovery API at `base_url`.
    pub fn with_base_url(api_key: Secret, base_url: impl Into<String>) -> Self {
        Self::with_http(api_key, base_url, Ureq::new(), MIN_INTERVAL)
    }

    fn with_http(
        api_key: Secret,
        base_url: impl Into<String>,
        http: impl Http + 'static,
        min_interval: Duration,
    ) -> Self {
        Self {
            client: Arc::new(Client {
                base_url: base_url.into().trim_end_matches('/').to_owned(),
                api_key,
                http: Box::new(http),
                limiter: RateLimiter::new(min_interval),
            }),
        }
    }

    /// The venue a Ticketmaster event is at, looked up by event ID.
    ///
    /// Venue pages such as Culture Room's link each show to Ticketmaster;
    /// the ID from that link resolves straight to the venue's Ticketmaster
    /// ID, with no name matching.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidEventId`] if `event_id` is not a bare ID,
    /// [`Error::Status`] with 404 for an unknown event, and the usual
    /// request errors.
    pub async fn venue_for_event(&self, event_id: &str) -> super::Result<Venue> {
        let bare = !event_id.is_empty()
            && event_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !bare {
            return Err(Error::InvalidEventId.into());
        }
        let client = Arc::clone(&self.client);
        let path = format!("/discovery/v2/events/{event_id}.json");
        run_blocking(move || {
            let body = client.get(&path, &[])?;
            api::parse_event_venue(&body).map_err(Error::Response)
        })
        .await
    }
}

impl fmt::Debug for Ticketmaster {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ticketmaster")
            .field("base_url", &self.client.base_url)
            .finish_non_exhaustive()
    }
}

impl EventProvider for Ticketmaster {
    async fn venues_near(&self, loc: &Location, radius_km: u32) -> super::Result<Vec<Venue>> {
        let client = Arc::clone(&self.client);
        let loc = loc.clone();
        run_blocking(move || client.venues_near(&loc, radius_km)).await
    }

    async fn upcoming_events(
        &self,
        _venue: &VenueId,
        _window: DateRange,
    ) -> super::Result<Vec<Event>> {
        // Ticketmaster events are #7.
        Err(Error::Response("Ticketmaster events are not supported yet".into()).into())
    }
}

/// Runs a blocking client call on tokio's blocking pool.
async fn run_blocking<T: Send + 'static>(
    call: impl FnOnce() -> Result<T, Error> + Send + 'static,
) -> super::Result<T> {
    match tokio::task::spawn_blocking(call).await {
        Ok(result) => result.map_err(Into::into),
        Err(join) => Err(super::Error::Other(Box::new(join))),
    }
}

/// Minimum time between requests: 5 per second.
const MIN_INTERVAL: Duration = Duration::from_millis(200);

/// Spaces requests at least `min_interval` apart across threads.
struct RateLimiter {
    min_interval: Duration,
    last: Mutex<Option<Instant>>,
}

impl RateLimiter {
    fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            last: Mutex::new(None),
        }
    }

    /// Blocks until a request may be sent, then records it as sent.
    ///
    /// Holds the lock while sleeping, so concurrent callers queue up
    /// rather than all firing once the interval passes.
    fn wait(&self) {
        let mut last = self
            .last
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(previous) = *last {
            let ready_at = previous + self.min_interval;
            let now = Instant::now();
            if ready_at > now {
                std::thread::sleep(ready_at - now);
            }
        }
        *last = Some(Instant::now());
    }
}

/// The blocking half of [`Ticketmaster`].
struct Client {
    base_url: String,
    api_key: Secret,
    http: Box<dyn Http>,
    limiter: RateLimiter,
}

impl Client {
    /// GETs `path` with the API key added, mapping error statuses.
    fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<String, Error> {
        self.limiter.wait();
        let url = format!("{}{path}", self.base_url);
        let mut query = query.to_vec();
        query.push(("apikey", self.api_key.expose()));
        let response = self.http.get(&url, &query).map_err(Error::Request)?;
        match response.status {
            200..=299 => Ok(response.body),
            429 => Err(Error::RateLimited {
                retry_after: response.retry_after,
            }),
            status => Err(Error::Status(status)),
        }
    }

    /// Every venue the API lists within `radius_km` of `loc`, across all
    /// pages, nearest first.
    fn venues_near(&self, loc: &Location, radius_km: u32) -> Result<Vec<Venue>, Error> {
        let latlong = format!("{},{}", loc.lat, loc.lon);
        let radius = radius_km.to_string();
        let size = PAGE_SIZE.to_string();
        let max_pages = MAX_RESULTS / PAGE_SIZE;

        let mut venues = Vec::new();
        let mut page = 0;
        loop {
            let number = page.to_string();
            let query = [
                ("latlong", latlong.as_str()),
                ("radius", radius.as_str()),
                ("unit", "km"),
                ("size", size.as_str()),
                ("page", number.as_str()),
                ("sort", "distance,asc"),
            ];
            let body = self.get("/discovery/v2/venues.json", &query)?;
            let parsed = api::parse_venue_page(&body).map_err(Error::Response)?;
            venues.extend(parsed.venues);
            page += 1;
            if page >= parsed.total_pages.min(max_pages) {
                break;
            }
        }

        // The API's own distance sort and radius are close but not exact;
        // apply ours so every provider agrees on what "within" means.
        let radius_km = f64::from(radius_km);
        let mut nearby: Vec<(f64, Venue)> = venues
            .into_iter()
            .map(|venue| (loc.distance_km(&venue.location), venue))
            .filter(|(km, _)| *km <= radius_km)
            .collect();
        nearby.sort_by(|(a, _), (b, _)| a.total_cmp(b));
        Ok(nearby.into_iter().map(|(_, venue)| venue).collect())
    }
}

#[cfg(test)]
mod tests {
    use Instant;
    use Mutex;

    use super::http::Response;
    use super::*;

    const PAGE_0: &str = include_str!("../../../tests/fixtures/ticketmaster/venues-page-0.json");
    const PAGE_1: &str = include_str!("../../../tests/fixtures/ticketmaster/venues-page-1.json");
    const EMPTY: &str = include_str!("../../../tests/fixtures/ticketmaster/venues-empty.json");

    const EVENT: &str = include_str!("../../../tests/fixtures/ticketmaster/event.json");

    #[tokio::test]
    async fn venue_for_event_looks_up_the_event() {
        let http = FakeHttp::serving(&[EVENT]);

        let venue = provider(&http)
            .venue_for_event("Z7r9jZ1ATest")
            .await
            .unwrap();

        assert_eq!(venue.id.to_string(), "ticketmaster:KovZpZHrlTest");
        let (url, _) = &http.requests()[0];
        assert_eq!(url, "https://tm.test/discovery/v2/events/Z7r9jZ1ATest.json");
        assert_eq!(http.param("apikey"), [KEY]);
    }

    #[tokio::test]
    async fn unknown_event_is_a_404() {
        let http = FakeHttp::default();
        http.push(404, None, r#"{"errors":[{"code":"DIS1004"}]}"#);

        let err = provider(&http).venue_for_event("nope").await.unwrap_err();

        assert_eq!(ticketmaster_error(err), Error::Status(404));
    }

    #[tokio::test]
    async fn event_id_that_is_not_a_bare_id_is_rejected_without_a_request() {
        let http = FakeHttp::default();

        for bad in ["", "../venues", "abc?x=1", "a/b"] {
            let err = provider(&http).venue_for_event(bad).await.unwrap_err();
            assert_eq!(ticketmaster_error(err), Error::InvalidEventId, "{bad:?}");
        }
        assert!(http.requests().is_empty());
    }

    const KEY: &str = "tm-test-key";

    /// A recorded request: URL and query parameters.
    type Request = (String, Vec<(String, String)>);

    /// Serves canned responses in order and records every request.
    #[derive(Clone, Default)]
    struct FakeHttp {
        responses: Arc<Mutex<Vec<Response>>>,
        requests: Arc<Mutex<Vec<Request>>>,
    }

    impl FakeHttp {
        fn serving(bodies: &[&str]) -> Self {
            let fake = Self::default();
            for body in bodies {
                fake.push(200, None, body);
            }
            fake
        }

        fn push(&self, status: u16, retry_after: Option<u64>, body: &str) {
            self.responses.lock().unwrap().push(Response {
                status,
                retry_after,
                body: body.into(),
            });
        }

        fn requests(&self) -> Vec<Request> {
            self.requests.lock().unwrap().clone()
        }

        /// The value of `name` in each request's query, in order.
        fn param(&self, name: &str) -> Vec<String> {
            self.requests()
                .into_iter()
                .filter_map(|(_, query)| {
                    query
                        .into_iter()
                        .find(|(key, _)| key == name)
                        .map(|(_, value)| value)
                })
                .collect()
        }
    }

    impl Http for FakeHttp {
        fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<Response, String> {
            self.requests.lock().unwrap().push((
                url.into(),
                query
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ));
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                return Err("no more canned responses".into());
            }
            Ok(responses.remove(0))
        }
    }

    fn provider(http: &FakeHttp) -> Ticketmaster {
        Ticketmaster::with_http(
            Secret::new(KEY),
            "https://tm.test/",
            http.clone(),
            Duration::ZERO,
        )
    }

    fn fort_lauderdale() -> Location {
        Location {
            lat: 26.1224,
            lon: -80.1373,
            label: "Fort Lauderdale, FL".into(),
        }
    }

    fn names(venues: &[Venue]) -> Vec<&str> {
        venues.iter().map(|v| v.name.as_str()).collect()
    }

    #[tokio::test]
    async fn venues_near_sends_a_geo_search_with_the_key() {
        let http = FakeHttp::serving(&[EMPTY]);

        provider(&http)
            .venues_near(&fort_lauderdale(), 40)
            .await
            .unwrap();

        let (url, _) = &http.requests()[0];
        assert_eq!(url, "https://tm.test/discovery/v2/venues.json");
        assert_eq!(http.param("latlong"), ["26.1224,-80.1373"]);
        assert_eq!(http.param("radius"), ["40"]);
        assert_eq!(http.param("unit"), ["km"]);
        assert_eq!(http.param("apikey"), [KEY]);
    }

    #[tokio::test]
    async fn venues_near_reads_every_page_nearest_first() {
        let http = FakeHttp::serving(&[PAGE_0, PAGE_1]);

        let venues = provider(&http)
            .venues_near(&fort_lauderdale(), 80)
            .await
            .unwrap();

        assert_eq!(http.param("page"), ["0", "1"]);
        assert_eq!(
            names(&venues),
            [
                "Revolution Live",
                "Culture Room",
                "The Fillmore Miami Beach",
                "iTHINK Financial Amphitheatre",
            ]
        );
    }

    #[tokio::test]
    async fn venues_near_drops_venues_outside_the_radius() {
        let http = FakeHttp::serving(&[PAGE_0, PAGE_1]);

        let venues = provider(&http)
            .venues_near(&fort_lauderdale(), 50)
            .await
            .unwrap();

        assert!(!names(&venues).contains(&"iTHINK Financial Amphitheatre"));
    }

    #[tokio::test]
    async fn venues_near_stops_at_the_deep_paging_limit() {
        let deep = PAGE_0.replace(r#""totalPages": 2"#, r#""totalPages": 40"#);
        let http = FakeHttp::serving(&[deep.as_str(); 10]);

        provider(&http)
            .venues_near(&fort_lauderdale(), 80)
            .await
            .unwrap();

        assert_eq!(http.param("page"), ["0", "1", "2", "3", "4"]);
        assert_eq!(http.param("size"), ["200"; 5]);
    }

    /// Unwraps a provider error back into a Ticketmaster [`Error`].
    fn ticketmaster_error(err: super::super::Error) -> Error {
        match err {
            super::super::Error::Other(inner) => *inner.downcast::<Error>().unwrap(),
            other => panic!("expected a Ticketmaster error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn http_429_is_a_rate_limit_error() {
        let http = FakeHttp::default();
        http.push(
            429,
            Some(3),
            r#"{"fault":{"faultstring":"Rate limit quota violation"}}"#,
        );

        let err = provider(&http)
            .venues_near(&fort_lauderdale(), 40)
            .await
            .unwrap_err();

        let shown = err.to_string();
        assert_eq!(
            ticketmaster_error(err),
            Error::RateLimited {
                retry_after: Some(3)
            }
        );
        assert!(shown.contains("rate limit"), "{shown}");
        assert!(shown.contains("429"), "{shown}");
    }

    #[tokio::test]
    async fn rate_limit_on_a_later_page_fails_the_search() {
        let http = FakeHttp::serving(&[PAGE_0]);
        http.push(429, None, "");

        let err = provider(&http)
            .venues_near(&fort_lauderdale(), 80)
            .await
            .unwrap_err();

        assert_eq!(
            ticketmaster_error(err),
            Error::RateLimited { retry_after: None }
        );
    }

    #[tokio::test]
    async fn other_error_statuses_are_reported_without_the_key() {
        let http = FakeHttp::default();
        http.push(401, None, r#"{"fault":{"faultstring":"Invalid ApiKey"}}"#);

        let err = provider(&http)
            .venues_near(&fort_lauderdale(), 40)
            .await
            .unwrap_err();

        let shown = format!("{err} {err:?}");
        assert!(!shown.contains(KEY), "leaked: {shown}");
        assert_eq!(ticketmaster_error(err), Error::Status(401));
    }

    #[tokio::test]
    async fn debug_output_does_not_show_the_key() {
        let shown = format!("{:?}", provider(&FakeHttp::default()));

        assert!(!shown.contains(KEY), "leaked: {shown}");
    }

    #[test]
    fn rate_limiter_spaces_requests_apart() {
        let limiter = RateLimiter::new(Duration::from_millis(40));

        let start = Instant::now();
        for _ in 0..3 {
            limiter.wait();
        }

        assert!(start.elapsed() >= Duration::from_millis(80));
    }

    #[test]
    fn rate_limiter_does_not_delay_the_first_request() {
        let limiter = RateLimiter::new(Duration::from_secs(10));

        let start = Instant::now();
        limiter.wait();

        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn default_rate_is_five_requests_per_second() {
        assert_eq!(MIN_INTERVAL, Duration::from_millis(200));
    }
}
