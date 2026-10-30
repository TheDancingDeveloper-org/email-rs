//! The check made on the server URL before the client talks to it: the
//! password and bearer token must not cross the network in the clear.

use email_client::is_https_or_loopback;
use url::Url;

/// Accepts an `https://` URL, or an `http://` URL whose host is loopback
/// (`localhost`, `127.0.0.0/8`, `::1`). Returns the message to show
/// otherwise.
///
/// The URL is parsed with the `url` crate, the WHATWG parser reqwest uses,
/// so the host checked here is the host reqwest connects to.
pub fn check_server_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    let parsed = Url::parse(url)
        .map_err(|e| format!("`{url}` is not a URL ({e}); expected https://host[:port]"))?;
    if parsed.host().is_none() {
        return Err(format!("`{url}` has no host"));
    }
    match parsed.scheme() {
        _ if is_https_or_loopback(&parsed) => Ok(()),
        "http" => Err(format!(
            "`{url}` uses plain http to a remote host; use https:// \
             (plain http is allowed only for localhost, 127.0.0.0/8 and ::1)"
        )),
        scheme => Err(format!(
            "`{url}` has scheme `{scheme}`; expected https:// (or http:// to loopback)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::check_server_url;

    #[test]
    fn https_is_accepted_for_any_host() {
        assert!(check_server_url("https://mail.example.com").is_ok());
        assert!(check_server_url("HTTPS://mail.example.com:8443/api").is_ok());
    }

    #[test]
    fn plain_http_is_accepted_for_loopback() {
        for url in [
            "http://localhost:8585",
            "http://LOCALHOST",
            "http://127.0.0.1:8585",
            "http://127.1.2.3/",
            "http://[::1]:8585",
            "http://user@localhost:8585",
            "HTTP://localhost:8585",
        ] {
            assert!(check_server_url(url).is_ok(), "{url}");
        }
    }

    #[test]
    fn plain_http_to_a_remote_host_is_rejected() {
        for url in [
            "http://mail.example.com",
            "http://192.168.1.10:8585",
            "http://[2001:db8::1]:8585",
            "http://localhost.example.com",
            "http://localhost@evil.example.com",
            "http://128.0.0.1",
            // The url crate (and reqwest) read `\` as `/`: the host is
            // evil.example.com, not localhost.
            "http://evil.example.com\\@localhost:8585",
            // Tab and newline are stripped, joining the host back up.
            "http://localhost\t.evil.example.com",
            "http://localhost\n.evil.example.com",
            "HTTP://mail.example.com",
        ] {
            let err = check_server_url(url).unwrap_err();
            assert!(err.contains("https://"), "{url}: {err}");
        }
    }

    #[test]
    fn malformed_urls_are_rejected() {
        assert!(check_server_url("mail.example.com").is_err());
        assert!(check_server_url("http://").is_err());
        assert!(check_server_url("ftp://localhost").is_err());
        // A percent-encoded `@` decodes to a forbidden host code point.
        assert!(check_server_url("http://evil.example.com%40localhost:8585").is_err());
    }
}
