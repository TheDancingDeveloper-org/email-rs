# Account discovery

`POST /api/v1/accounts/discover` accepts `{"email":"user@example.com"}`.
Application sessions or integration tokens with `mail.accounts.connect` may call it.
It returns `email`, normalized `domain`, and `candidates`. Each candidate has
`providerType`, `source`, `imap` and `smtp`; endpoints contain `host`, `port`,
and `security` (`ssl` or `starttls`). No account is created or credentials tested.

Discovery consults Thunderbird ISPDB, domain `.well-known/autoconfig/mail/config-v1.1.xml`,
and `autoconfig.DOMAIN/mail/config-v1.1.xml` over HTTPS, then RFC 6186
`_imaps._tcp` and `_submission._tcp`. Known Google and Microsoft MX suffixes
provide heuristic suggestions when SRV cannot provide a complete pair. Unknown
MX records are not treated as submission servers. Empty candidates mean manual
setup is needed; network failures are best-effort misses, not account failures.

Candidates are suggestions requiring user confirmation, not verified credentials.
No email local part is sent to discovery services. HTTP redirects and proxies are
disabled, resolved public IPv4 addresses are pinned, and XML bodies are capped at
256 KiB. IPv6-only autoconfig hosts are conservatively skipped. Each HTTP/DNS
resolution has a timeout and the SRV/MX phase is capped at eight seconds.
Plaintext XML endpoints, unsupported username templates and authentication methods
are not advertised as supported account credentials; consumers must still complete
their normal authentication setup. This endpoint does not implement desktop #87.
