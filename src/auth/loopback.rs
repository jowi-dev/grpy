//! The loopback redirect: a one-shot HTTP listener on `127.0.0.1` that
//! receives the authorization code from the browser.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpListener;

use url::Url;

use super::AuthError;

/// Listens on a free port on `127.0.0.1` for Google's redirect.
pub struct Loopback {
    listener: TcpListener,
}

impl Loopback {
    /// Binds a free port on the loopback interface.
    ///
    /// # Errors
    ///
    /// When no port can be bound.
    pub fn bind() -> io::Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(("127.0.0.1", 0))?,
        })
    }

    /// The `redirect_uri` to give Google: `http://127.0.0.1:<port>/`.
    ///
    /// # Errors
    ///
    /// When the bound port can't be read back.
    pub fn redirect_uri(&self) -> io::Result<String> {
        Ok(format!(
            "http://127.0.0.1:{}/",
            self.listener.local_addr()?.port()
        ))
    }

    /// Serves requests until the browser arrives with an authorization
    /// code or an error, and returns the code. Other requests, such as
    /// for `/favicon.ico`, get a 404.
    ///
    /// # Errors
    ///
    /// As [`handle_request`], or [`AuthError::Io`] when accepting a
    /// connection fails.
    pub fn wait_for_code(&self, expected_state: &str) -> Result<String, AuthError> {
        loop {
            let (stream, _) = self
                .listener
                .accept()
                .map_err(|err| AuthError::Io(err.to_string()))?;
            if let Some(code) = handle_request(stream, expected_state)? {
                return Ok(code);
            }
        }
    }
}

/// Reads one HTTP request from `stream`, answers it, and returns the
/// authorization code if this was Google's redirect.
///
/// Returns `Ok(None)` for requests that aren't the redirect, so the caller
/// keeps waiting.
///
/// # Errors
///
/// [`AuthError::StateMismatch`] when the redirect's `state` isn't
/// `expected_state`, [`AuthError::Denied`] when the user declined, and
/// [`AuthError::OAuth`] for other errors Google redirects with.
pub fn handle_request(
    mut stream: impl Read + Write,
    expected_state: &str,
) -> Result<Option<String>, AuthError> {
    let io_error = |err: io::Error| AuthError::Io(err.to_string());

    let mut request_line = String::new();
    let mut reader = BufReader::new(&mut stream);
    reader.read_line(&mut request_line).map_err(io_error)?;
    // Drain the headers so closing the connection doesn't reset it before
    // the browser reads the response.
    let mut header = String::new();
    while reader.read_line(&mut header).map_err(io_error)? > 2 {
        header.clear();
    }

    let Some(target) = request_line.split(' ').nth(1) else {
        return Ok(None);
    };
    let Ok(url) = Url::parse("http://127.0.0.1").and_then(|base| base.join(target)) else {
        return respond(&mut stream, 400, "Bad request.").map(|()| None);
    };
    let param = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };

    let (code, error) = (param("code"), param("error"));
    if code.is_none() && error.is_none() {
        return respond(&mut stream, 404, "Not found.").map(|()| None);
    }
    if param("state").as_deref() != Some(expected_state) {
        respond(
            &mut stream,
            400,
            "This sign-in response doesn't match the one grpy started. \
             Run <code>grpy auth google</code> again.",
        )?;
        return Err(AuthError::StateMismatch);
    }
    if let Some(error) = error {
        respond(
            &mut stream,
            200,
            "Sign-in was not completed. You can close this tab and return to the terminal.",
        )?;
        return Err(match error.as_str() {
            "access_denied" => AuthError::Denied,
            _ => AuthError::OAuth {
                error,
                description: param("error_description"),
            },
        });
    }
    respond(
        &mut stream,
        200,
        "grpy is signed in to Google Calendar. You can close this tab and return to the terminal.",
    )?;
    Ok(code)
}

/// Writes a small HTML page and closes the exchange.
fn respond(stream: &mut impl Write, status: u16, message: &str) -> Result<(), AuthError> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>grpy</title></head>\
         <body><p>{message}</p></body></html>"
    );
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .and_then(|()| stream.flush())
    .map_err(|err| AuthError::Io(err.to_string()))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    /// A connection whose request is `input` and whose response is
    /// collected in `output`.
    struct FakeStream {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
    }

    impl FakeStream {
        fn get(target: &str) -> Self {
            Self {
                input: Cursor::new(
                    format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:8765\r\n\r\n").into_bytes(),
                ),
                output: Vec::new(),
            }
        }

        fn response(&self) -> String {
            String::from_utf8_lossy(&self.output).into_owned()
        }
    }

    impl Read for FakeStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.input.read(buf)
        }
    }

    impl Write for FakeStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.output.write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn redirect_with_code_and_matching_state_returns_the_code() {
        let mut stream = FakeStream::get("/?state=s123&code=4%2F0Abc&scope=x");

        let code = handle_request(&mut stream, "s123").unwrap();

        assert_eq!(code.as_deref(), Some("4/0Abc"));
        let response = stream.response();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains("terminal"), "{response}");
    }

    #[test]
    fn mismatched_state_is_rejected() {
        let mut stream = FakeStream::get("/?state=forged&code=4%2F0Abc");

        assert_eq!(
            handle_request(&mut stream, "s123"),
            Err(AuthError::StateMismatch)
        );
        assert!(stream.response().starts_with("HTTP/1.1 400"));
    }

    #[test]
    fn missing_state_is_rejected() {
        let mut stream = FakeStream::get("/?code=4%2F0Abc");

        assert_eq!(
            handle_request(&mut stream, "s123"),
            Err(AuthError::StateMismatch)
        );
    }

    #[test]
    fn declined_consent_is_denied() {
        let mut stream = FakeStream::get("/?error=access_denied&state=s123");

        assert_eq!(handle_request(&mut stream, "s123"), Err(AuthError::Denied));
        assert!(stream.response().starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn other_redirect_errors_are_oauth_errors() {
        let mut stream = FakeStream::get("/?error=invalid_scope&state=s123");

        assert_eq!(
            handle_request(&mut stream, "s123"),
            Err(AuthError::OAuth {
                error: "invalid_scope".into(),
                description: None,
            })
        );
    }

    #[test]
    fn unrelated_requests_get_a_404_and_are_ignored() {
        let mut stream = FakeStream::get("/favicon.ico");

        assert_eq!(handle_request(&mut stream, "s123"), Ok(None));
        assert!(stream.response().starts_with("HTTP/1.1 404"));
    }

    #[test]
    fn empty_connections_are_ignored() {
        let mut stream = FakeStream {
            input: Cursor::new(Vec::new()),
            output: Vec::new(),
        };

        assert_eq!(handle_request(&mut stream, "s123"), Ok(None));
    }
}
