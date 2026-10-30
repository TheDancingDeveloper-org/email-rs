//! The check made on the server URL before the client talks to it: the
//! password and bearer token must not cross the network in the clear.

use std::net::{Ipv4Addr, Ipv6Addr};

/// Accepts an `https://` URL, or an `http://` URL whose host is loopback
/// (`localhost`, `127.0.0.0/8`, `::1`). Returns the message to show
/// otherwise.
pub fn check_server_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| format!("`{url}` is not a URL; expected https://host[:port]"))?;
    let host = host_of(rest).ok_or_else(|| format!("`{url}` has no host"))?;
    match scheme.to_ascii_lowercase().as_str() {
        "https" => Ok(()),
        "http" if is_loopback(host) => Ok(()),
        "http" => Err(format!(
            "`{url}` uses plain http to a remote host; use https:// \
             (plain http is allowed only for localhost, 127.0.0.0/8 and ::1)"
        )),
        _ => Err(format!(
            "`{url}` has scheme `{scheme}`; expected https:// (or http:// to loopback)"
        )),
    }
}

/// The host part of `authority[/path...]`, without userinfo, port or the
/// brackets around an IPv6 literal.
fn host_of(rest: &str) -> Option<&str> {
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if let Some(bracketed) = host_port.strip_prefix('[') {
        bracketed.split_once(']')?.0
    } else {
        host_port.split(':').next().unwrap_or("")
    };
    (!host.is_empty()).then_some(host)
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<Ipv4Addr>().is_ok_and(|ip| ip.is_loopback())
        || host.parse::<Ipv6Addr>().is_ok_and(|ip| ip.is_loopback())
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
    }
}
