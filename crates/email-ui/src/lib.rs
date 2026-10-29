//! F0 spike skeleton for the fluent2-egui client.
//!
//! Nothing here is on the default workspace build. Enable the `spike`
//! feature to compile the fixture backend and the HtmlView lossy renderer.
//! The real eframe + fluent2-egui shell is deferred until the operator
//! go/no-go (see `docs/spikes/F0.md`).

#![cfg(feature = "spike")]

/// Read-only view the shell needs. No I/O.
pub trait Backend {
    fn messages(&self) -> &[FixtureMail];
}

#[derive(Clone, Debug)]
pub struct FixtureMail {
    pub id: &'static str,
    pub subject: &'static str,
    pub html: &'static str,
}

pub struct FixtureBackend {
    mails: Vec<FixtureMail>,
}

impl FixtureBackend {
    pub fn ten() -> Self {
        Self {
            mails: TEN.to_vec(),
        }
    }
}

impl Backend for FixtureBackend {
    fn messages(&self) -> &[FixtureMail] {
        &self.mails
    }
}

/// Lossy block model: keep text, drop tags. Good enough to judge whether
/// ten fixture bodies stay readable. Not an HTML engine.
pub fn render_plain(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn render_all(backend: &impl Backend) -> Vec<String> {
    backend
        .messages()
        .iter()
        .map(|m| render_plain(m.html))
        .collect()
}

const TEN: &[FixtureMail] = &[
    FixtureMail { id: "1", subject: "Plain", html: "Hello from plain text." },
    FixtureMail { id: "2", subject: "Bold", html: "<p>Hello <b>world</b>.</p>" },
    FixtureMail { id: "3", subject: "Link", html: "<p>See <a href=\"https://example.test\">docs</a>.</p>" },
    FixtureMail { id: "4", subject: "List", html: "<ul><li>one</li><li>two</li></ul>" },
    FixtureMail { id: "5", subject: "Quote", html: "<blockquote>quoted line</blockquote>" },
    FixtureMail { id: "6", subject: "Pre", html: "<pre>code block</pre>" },
    FixtureMail { id: "7", subject: "Table", html: "<table><tr><td>A</td><td>B</td></tr></table>" },
    FixtureMail { id: "8", subject: "Outlook", html: "<div class=\"WordSection1\"><p>Outlook <span style=\"color:red\">reply</span></p></div>" },
    FixtureMail { id: "9", subject: "Newsletter", html: "<table width=\"600\"><tr><td><h1>Sale</h1><p>Shop now</p></td></tr></table>" },
    FixtureMail { id: "10", subject: "Empty", html: "" },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_fixture_mails_render_readable_text() {
        let backend = FixtureBackend::ten();
        assert_eq!(backend.messages().len(), 10);
        let rendered = render_all(&backend);
        assert!(rendered[1].contains("Hello world"));
        assert!(rendered[2].contains("docs"));
        assert!(rendered[6].contains("A") && rendered[6].contains("B"));
        assert!(rendered[7].contains("Outlook reply"));
        assert!(rendered[9].is_empty());
        assert!(rendered.iter().all(|r| !r.contains('<')));
    }
}
