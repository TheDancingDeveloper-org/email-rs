const DEPLOYMENT_SECURITY: &str = include_str!("../../../docs/deployment-security.md");

#[test]
fn production_caddy_example_documents_security_boundary() {
    for required in [
        "mail.example.com {",
        "Strict-Transport-Security",
        "Content-Security-Policy",
        "X-Content-Type-Options \"nosniff\"",
        "Referrer-Policy",
        "Permissions-Policy",
        "reverse_proxy email-rs:3000",
        "X-Forwarded-Proto",
        "trusted_proxies_strict",
        "port 3000 must be reachable only from Caddy",
    ] {
        assert!(
            DEPLOYMENT_SECURITY.contains(required),
            "deployment security guide is missing {required:?}"
        );
    }
}
