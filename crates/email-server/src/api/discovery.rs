use std::time::Duration;

use axum::extract::Json;
use hickory_resolver::{name_server::TokioConnectionProvider, TokioResolver};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

const LOOKUP_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_BODY: usize = 256 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryRequest {
    pub email: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct MailEndpoint {
    pub host: String,
    pub port: u16,
    pub security: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct AccountCandidate {
    pub provider_type: String,
    pub imap: MailEndpoint,
    pub smtp: MailEndpoint,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryResponse {
    pub email: String,
    pub domain: String,
    pub candidates: Vec<AccountCandidate>,
}

pub async fn discover_account(
    Json(request): Json<DiscoveryRequest>,
) -> Result<axum::Json<DiscoveryResponse>> {
    let (email, domain) = validate_email(&request.email)?;
    let mut candidates = Vec::new();

    for (url, source) in discovery_urls(&domain) {
        if let Ok(body) = fetch_text(&url).await {
            candidates.extend(parse_autoconfig(&body, source));
        }
    }
    if let Ok(found) =
        tokio::time::timeout(Duration::from_secs(8), discover_srv_and_mx(&domain)).await
    {
        candidates.extend(found);
    }
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|candidate| seen.insert(candidate.clone()));

    Ok(Json(DiscoveryResponse {
        email,
        domain,
        candidates,
    }))
}

fn validate_email(input: &str) -> Result<(String, String)> {
    let email = input.trim();
    let Some((local, domain)) = email.rsplit_once('@') else {
        return Err(AppError::Validation("email must contain a domain".into()));
    };
    if local.is_empty()
        || local.len() > 64
        || local.contains('@')
        || email.chars().any(char::is_whitespace)
        || domain.is_empty()
        || domain.len() > 253
        || domain.contains(['/', ':', '\\', ' '])
        || valid_host(domain).is_none()
    {
        return Err(AppError::Validation("email address is invalid".into()));
    }
    Ok((email.to_owned(), domain.to_ascii_lowercase()))
}

fn discovery_urls(domain: &str) -> [(String, &'static str); 3] {
    [
        (
            format!("https://autoconfig.thunderbird.net/v1.1/{domain}"),
            "ispdb",
        ),
        (
            format!("https://{domain}/.well-known/autoconfig/mail/config-v1.1.xml"),
            "well-known",
        ),
        (
            format!("https://autoconfig.{domain}/mail/config-v1.1.xml"),
            "autoconfig",
        ),
    ]
}

async fn fetch_text(url: &str) -> anyhow::Result<String> {
    let url = reqwest::Url::parse(url)?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("missing host"))?;
    let addresses: Vec<_> =
        tokio::time::timeout(LOOKUP_TIMEOUT, tokio::net::lookup_host((host, 443)))
            .await??
            .collect();
    anyhow::ensure!(
        !addresses.is_empty() && addresses.iter().all(|address| public_ip(address.ip())),
        "non-public address"
    );
    let client = reqwest::Client::builder()
        .no_proxy()
        .resolve_to_addrs(host, &addresses)
        .connect_timeout(LOOKUP_TIMEOUT)
        .timeout(LOOKUP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut response = client.get(url).send().await?.error_for_status()?;
    anyhow::ensure!(response.status().is_success(), "unexpected status");
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(bytes.len() + chunk.len() <= MAX_BODY, "response too large");
        bytes.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8(bytes)?)
}

fn parse_autoconfig(body: &str, source: &str) -> Vec<AccountCandidate> {
    let Ok(document) = roxmltree::Document::parse(body) else {
        return Vec::new();
    };
    let find = |tag, protocol| {
        document
            .descendants()
            .filter(|node| node.has_tag_name(tag) && node.attribute("type") == Some(protocol))
            .find_map(endpoint)
    };
    let (Some(imap), Some(smtp)) = (
        find("incomingServer", "imap"),
        find("outgoingServer", "smtp"),
    ) else {
        return Vec::new();
    };
    vec![AccountCandidate {
        provider_type: "imap".into(),
        imap,
        smtp,
        source: source.into(),
    }]
}

fn endpoint(node: roxmltree::Node<'_, '_>) -> Option<MailEndpoint> {
    let text = |tag| {
        node.children()
            .find(|child| child.has_tag_name(tag))
            .and_then(|child| child.text())
            .map(str::trim)
    };
    let port = text("port")?.parse().ok().filter(|port| *port != 0)?;
    let security = text("socketType")?.to_ascii_lowercase();
    if !matches!(security.as_str(), "ssl" | "starttls") {
        return None;
    }
    Some(MailEndpoint {
        host: valid_host(text("hostname")?)?,
        port,
        security,
    })
}

fn valid_host(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    (host.len() <= 253
        && host.contains('.')
        && host.parse::<std::net::IpAddr>().is_err()
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        }))
    .then_some(host)
}

fn public_ip(address: std::net::IpAddr) -> bool {
    match address {
        std::net::IpAddr::V4(ip) => {
            let octets = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_documentation()
                && !ip.is_broadcast()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && octets[0] != 0
                && octets[0] < 224
                && !(octets[0] == 100 && (64..=127).contains(&octets[1]))
                && !(octets[0] == 198 && (18..=19).contains(&octets[1]))
        }
        std::net::IpAddr::V6(_) => false,
    }
}

async fn discover_srv_and_mx(domain: &str) -> Vec<AccountCandidate> {
    let resolver = TokioResolver::builder(TokioConnectionProvider::default())
        .ok()
        .map(|builder| builder.build());
    let Some(resolver) = resolver else {
        return Vec::new();
    };
    let imap = resolver
        .srv_lookup(format!("_imaps._tcp.{domain}"))
        .await
        .ok()
        .and_then(|lookup| {
            lookup
                .iter()
                .min_by_key(|record| record.priority())
                .map(|record| MailEndpoint {
                    host: valid_host(record.target().to_utf8().as_str()).unwrap_or_default(),
                    port: record.port(),
                    security: "ssl".into(),
                })
        });
    let smtp = resolver
        .srv_lookup(format!("_submission._tcp.{domain}"))
        .await
        .ok()
        .and_then(|lookup| {
            lookup
                .iter()
                .min_by_key(|record| record.priority())
                .map(|record| MailEndpoint {
                    host: valid_host(record.target().to_utf8().as_str()).unwrap_or_default(),
                    port: record.port(),
                    security: "starttls".into(),
                })
        });
    match (
        imap.filter(|endpoint| !endpoint.host.is_empty() && endpoint.port != 0),
        smtp.filter(|endpoint| !endpoint.host.is_empty() && endpoint.port != 0),
    ) {
        (Some(imap), Some(smtp)) => vec![AccountCandidate {
            provider_type: "imap".into(),
            imap,
            smtp,
            source: "srv".into(),
        }],
        _ => resolver
            .mx_lookup(domain)
            .await
            .ok()
            .and_then(|lookup| {
                lookup
                    .iter()
                    .min_by_key(|record| record.preference())
                    .and_then(|record| mx_candidate(&record.exchange().to_utf8()))
            })
            .into_iter()
            .collect(),
    }
}

fn mx_candidate(exchange: &str) -> Option<AccountCandidate> {
    let exchange = exchange.trim_end_matches('.').to_ascii_lowercase();
    let (provider, imap, smtp) = if exchange == "aspmx.l.google.com"
        || exchange.ends_with(".aspmx.l.google.com")
        || exchange == "smtp.google.com"
    {
        ("gmail", "imap.gmail.com", "smtp.gmail.com")
    } else if exchange.ends_with(".mail.protection.outlook.com") {
        (
            "microsoft365",
            "outlook.office365.com",
            "smtp.office365.com",
        )
    } else {
        return None;
    };
    Some(AccountCandidate {
        provider_type: provider.into(),
        imap: MailEndpoint {
            host: imap.into(),
            port: 993,
            security: "ssl".into(),
        },
        smtp: MailEndpoint {
            host: smtp.into(),
            port: 587,
            security: "starttls".into(),
        },
        source: "mx-heuristic".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_addresses_and_unknown_mx() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "::1",
        ] {
            assert!(!public_ip(address.parse().unwrap()));
        }
        for email in [
            "a@127.0.0.1",
            "a@example.com?x",
            "a@foo@bar.com",
            "a@-foo.com",
        ] {
            assert!(validate_email(email).is_err());
        }
        assert!(mx_candidate("evilgoogle.com").is_none());
        assert!(mx_candidate("mx.example.com").is_none());
        assert_eq!(
            mx_candidate("aspmx.l.google.com.").unwrap().provider_type,
            "gmail"
        );
        assert_eq!(
            mx_candidate("tenant.mail.protection.outlook.com.")
                .unwrap()
                .smtp
                .port,
            587
        );
        assert!(parse_autoconfig("not xml", "ispdb").is_empty());
    }

    #[test]
    fn validates_and_normalizes_email() {
        assert_eq!(
            validate_email(" User@Example.COM ").unwrap(),
            ("User@Example.COM".into(), "example.com".into())
        );
        assert!(validate_email("missing-domain").is_err());
        assert!(validate_email("a@localhost").is_err());
    }

    #[test]
    fn parses_thunderbird_autoconfig() {
        let xml = r#"<clientConfig><emailProvider><incomingServer type="imap"><hostname>imap.example.test</hostname><port>993</port><socketType>SSL</socketType></incomingServer><outgoingServer type="smtp"><hostname>smtp.example.test</hostname><port>465</port><socketType>SSL</socketType></outgoingServer></emailProvider></clientConfig>"#;
        let result = parse_autoconfig(xml, "ispdb");
        assert_eq!(result[0].imap.host, "imap.example.test");
        assert_eq!(result[0].smtp.port, 465);
    }
}
