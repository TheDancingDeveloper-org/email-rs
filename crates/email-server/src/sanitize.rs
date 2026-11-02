//! Server-side allowlist HTML sanitizer for message bodies.
//!
//! Mirrors the client rules in `shared/sanitizeEmailHtml.ts`: no scripts,
//! event handlers, `javascript:` URLs or unsafe CSS, and remote images are
//! blocked by default. Clients still sanitize again as defence in depth.

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::LazyLock;

use ammonia::Builder;

static CSS_UNSAFE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?i)url\s*\(|expression\s*\(|@import|javascript:|behaviou?r\s*:|-moz-binding",
    )
    .expect("valid regex")
});

fn is_data_image(value: &str) -> bool {
    value
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("data:image/")
}

fn builder() -> Builder<'static> {
    let mut b = Builder::default();
    b.url_schemes(HashSet::from(["http", "https", "mailto", "data"]));
    b.clean_content_tags(HashSet::from(["script", "style"]));
    b.add_generic_attributes(["style"]);
    b.link_rel(Some("noopener noreferrer"));
    b.attribute_filter(|element, attribute, value| match (element, attribute) {
        (_, "style") if value.contains('\\') || CSS_UNSAFE.is_match(value) => None,
        // `data:` is allowed through the scheme list for inline images only.
        ("img", "src") if is_data_image(value) => Some(Cow::Borrowed(value)),
        ("img", "src") => None,
        (_, "href") if value.trim_start().to_ascii_lowercase().starts_with("data:") => None,
        _ => Some(Cow::Borrowed(value)),
    });
    b
}

/// Sanitize untrusted message HTML down to the allowlist.
pub fn sanitize_html(html: &str) -> String {
    builder().clean(html).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(html: &str) -> String {
        sanitize_html(html).to_ascii_lowercase()
    }

    #[test]
    fn strips_scripts_and_their_content() {
        let out = lower("<p>hi</p><script>alert(1)</script><SCRIPT src=x></SCRIPT>");
        assert!(!out.contains("script") && !out.contains("alert"));
        assert!(out.contains("<p>hi</p>"));
    }

    #[test]
    fn strips_event_handlers() {
        let out = lower(
            r#"<img src="data:image/png;base64,AA==" onerror="alert(1)"><a href="https://a.b" onclick="x()" onmouseover=y>l</a><body onload=z>"#,
        );
        assert!(!out.contains("onerror") && !out.contains("onclick") && !out.contains("onload"));
        assert!(!out.contains("onmouseover"));
    }

    #[test]
    fn strips_javascript_and_data_urls_from_links() {
        for href in [
            "javascript:alert(1)",
            " JaVaScRiPt:alert(1)",
            "java\tscript:alert(1)",
            "data:text/html,<script>1</script>",
            "vbscript:x",
        ] {
            let out = lower(&format!(r#"<a href="{href}">x</a>"#));
            assert!(
                !out.contains("javascript") && !out.contains("data:") && !out.contains("vbscript"),
                "{href}: {out}"
            );
        }
        assert!(sanitize_html(r#"<a href="https://example.com/">x</a>"#)
            .contains(r#"href="https://example.com/""#));
        assert!(sanitize_html(r#"<a href="mailto:a@b.c">x</a>"#).contains("mailto:a@b.c"));
    }

    #[test]
    fn strips_dangerous_elements() {
        let out = lower(
            r#"<iframe src="https://e.x"></iframe><object data=x></object><embed src=x><form action="/x"><input name=a><button>b</button></form><svg onload=alert(1)><script>1</script></svg><math><mi>x</mi></math><link rel=stylesheet href=x><meta http-equiv=refresh content="0;url=x"><base href="https://e.x">"#,
        );
        for tag in [
            "<iframe", "<object", "<embed", "<form", "<input", "<button", "<svg", "<math", "<link",
            "<meta", "<base",
        ] {
            assert!(!out.contains(tag), "{tag} survived: {out}");
        }
    }

    #[test]
    fn strips_style_elements_and_unsafe_css() {
        let out = lower(
            r#"<style>@import 'x';p{background:url(https://t/x)}</style><p style="background:url(https://t/x)">a</p><p style="width:expression(alert(1))">b</p><p style="color: red">c</p>"#,
        );
        assert!(
            !out.contains("<style")
                && !out.contains("url(")
                && !out.contains("expression")
                && !out.contains("@import")
        );
        assert!(out.contains("color: red"));
    }

    #[test]
    fn blocks_remote_images_but_keeps_inline_data_images() {
        let out = lower(
            r#"<img src="https://t.example/p.gif" alt="a"><img src="//t.example/p.gif"><img src="cid:abc"><img srcset="https://t/x 2x" src="http://t/x"><img src="data:image/png;base64,AA==">"#,
        );
        assert!(
            !out.contains("http")
                && !out.contains("t.example")
                && !out.contains("cid:")
                && !out.contains("srcset")
        );
        assert!(out.contains("data:image/png;base64,aa=="));
        assert!(!lower(r#"<img src="data:text/html,x">"#).contains("data:"));
    }

    #[test]
    fn links_get_noopener() {
        let out = sanitize_html(r#"<a href="https://a.b" target="_top">x</a>"#);
        assert!(out.contains(r#"rel="noopener noreferrer""#));
        assert!(!out.contains("_top"));
    }

    #[test]
    fn survives_malformed_nesting_tricks() {
        let out = lower(
            r#"<<script>script>alert(1)<</script>/script><img src=x:x onerror=alert(1)//><noscript><p title="</noscript><img src=x onerror=alert(1)>">"#,
        );
        assert!(
            !out.contains("<script") && !out.contains("onerror=") || !out.contains("<img src=x")
        );
        assert!(!out.contains("onerror=alert"));
    }
}
