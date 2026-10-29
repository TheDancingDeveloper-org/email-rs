//! Cleaned message bodies.
//!
//! Each message body is cleaned once, when it is cached, rather than by every
//! consumer that hands mail to a language model. The result is the *current*
//! message only, as Markdown:
//!
//! - real `text/plain` is preferred; an empty or "view in browser" stub falls
//!   back to the HTML part, converted with `htmd` (links and header-bearing
//!   tables survive, images, styles, scripts and hidden preheaders do not);
//! - quoted history (Gmail/Apple "On … wrote:", Outlook "From:/Sent:" blocks,
//!   "-----Original Message-----", trailing `>` quotes and their localized
//!   variants) is split off into `quoted_history`;
//! - signatures (`-- ` delimiter, Gmail/Outlook signature blocks, "Sent from my
//!   iPhone" lines) are dropped;
//! - tracking links lose their URL, `utm_*` style parameters are removed, and
//!   very long URLs are shortened to their host.

use std::rc::Rc;
use std::sync::LazyLock;

use htmd::element_handler::{HandlerResult, Handlers};
use htmd::{Element, HtmlToMarkdown};
use markup5ever_rcdom::{Node, NodeData};
use regex::{Captures, Regex};
use sqlx::SqlitePool;

/// Bump when the cleaning rules change; stored rows below it are recomputed.
pub const CLEAN_VERSION: i64 = 1;

/// Longest URL kept verbatim; longer ones are shortened to their host.
const MAX_URL_CHARS: usize = 100;
/// Characters of the clean body stored as the message preview.
const PREVIEW_CHARS: usize = 200;

// Private-use code points the HTML handlers leave in the Markdown so the text
// pass can find structural boundaries the HTML made explicit.
const QUOTE_MARK: &str = "\u{E000}QUOTE\u{E000}";
const SIG_MARK: &str = "\u{E000}SIG\u{E000}";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CleanBody {
    /// The current message as Markdown, without quoted history or signature.
    pub body: String,
    /// The quoted history split off the current message, if any.
    pub quoted_history: Option<String>,
    /// Whether a signature was removed from `body`.
    pub signature_stripped: bool,
}

/// Clean a message from its MIME text and HTML parts. `subject` only serves to
/// recognise forwards, whose "quoted" part is the content and is kept.
pub fn clean(text: Option<&str>, html: Option<&str>, subject: Option<&str>) -> CleanBody {
    use mail_parser::decoders::html::{html_to_text, text_to_html};

    // mail-parser synthesises the missing part of a single-part message: HTML
    // from plain text, or text from HTML. A synthesised HTML part adds nothing,
    // and a synthesised text part is worse than the HTML it came from (no
    // links, tables or quote markup), so each is ignored.
    let mut html = html.filter(|h| !h.trim().is_empty());
    let mut text = text;
    if let (Some(t), Some(h)) = (text, html) {
        if h == text_to_html(t) {
            html = None;
        } else if t.trim() == html_to_text(h).trim() {
            text = None;
        }
    }
    let source = match text {
        Some(t) if looks_like_html(t) => html_to_markdown(html.unwrap_or(t)),
        Some(t) if is_real_text(t, html.is_some()) => t.replace("\r\n", "\n"),
        _ => match html {
            Some(h) => html_to_markdown(h),
            None => text.unwrap_or_default().replace("\r\n", "\n"),
        },
    };
    let source = strip_invisible(&source);

    let forward = subject.is_some_and(|s| FORWARD_SUBJECT.is_match(s));
    let (current, quoted) = if forward {
        (source.clone(), None)
    } else {
        split_quoted(&source)
    };
    let (current, signature_stripped) = strip_signature(&current);
    let body = tidy(&clean_links(&current));
    let quoted_history = quoted
        .map(|q| tidy(&clean_links(&q)))
        .filter(|q| !q.is_empty());

    match quoted_history {
        // Nothing of its own (a bare forward or a quote-only reply): the history
        // is the content.
        Some(history) if body.is_empty() => CleanBody {
            body: history,
            quoted_history: None,
            signature_stripped,
        },
        quoted_history => CleanBody {
            body,
            quoted_history,
            signature_stripped,
        },
    }
}

/// A clean body as search-index text: links keep their label but not
/// their target, images go, and a bare URL is reduced to its host. URL paths
/// and query strings are only noise to full-text search.
pub fn search_text(body: &str) -> String {
    LINKISH
        .replace_all(body, |caps: &Captures| {
            if caps.get(2).is_some() {
                return caps.get(1).map_or("", |m| m.as_str()).to_string();
            }
            if let Some(url) = caps.get(3).or_else(|| caps.get(4)) {
                return url_host(url.as_str()).to_string();
            }
            String::new()
        })
        .into_owned()
}

/// A one-line preview of a clean body.
pub fn preview(body: &str) -> Option<String> {
    let flat = MD_LINK
        .replace_all(body, "$1")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let flat = flat.trim_start_matches(['#', '*', '>', ' ']);
    (!flat.is_empty()).then(|| flat.chars().take(PREVIEW_CHARS).collect())
}

// ---------------------------------------------------------------------------
// Choosing the source part
// ---------------------------------------------------------------------------

static STUB_PHRASE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        view\ (?:this|it|the|our)?\s*(?:e-?mail|message|newsletter|version)?\s*(?:in|on)\ (?:your|a|the)?\s*(?:web\ )?browser
        | (?:e-?mail|mail)\ (?:client|program|reader)\ (?:does\ not|doesn'?t|cannot|can'?t|is\ unable\ to)
        | this\ (?:is\ an\ html|message\ is\ in\ html|e-?mail\ requires\ html)
        | (?:to\ view|to\ read|to\ see)\ (?:this|the)\ (?:e-?mail|message|newsletter)
        | html[-\ ](?:version|capable|compatible)
        | open\ (?:this|it)\ in\ (?:a|your)\ browser",
    )
    .expect("stub regex")
});

fn looks_like_html(text: &str) -> bool {
    let head = text.trim_start().get(..200).unwrap_or(text.trim_start());
    let head = head.to_ascii_lowercase();
    head.starts_with("<!doctype html") || head.starts_with("<html") || head.contains("<body")
}

/// Whether a `text/plain` part carries the message rather than a stub.
fn is_real_text(text: &str, has_html: bool) -> bool {
    let visible = text.chars().filter(|c| !c.is_whitespace()).count();
    if visible == 0 {
        return false;
    }
    if !has_html {
        return true;
    }
    // A short part that only points at the HTML version.
    !(visible < 600 && STUB_PHRASE.is_match(text))
}

// ---------------------------------------------------------------------------
// HTML to Markdown
// ---------------------------------------------------------------------------

fn attr<'a>(element: &'a Element, name: &str) -> Option<&'a str> {
    element
        .attrs
        .iter()
        .find(|a| a.name.local.as_ref() == name)
        .map(|a| a.value.as_ref())
}

fn has_class(element: &Element, class: &str) -> bool {
    attr(element, "class").is_some_and(|c| c.split_whitespace().any(|c| c == class))
}

fn is_hidden(element: &Element) -> bool {
    if attr(element, "hidden").is_some() || attr(element, "aria-hidden") == Some("true") {
        return true;
    }
    let Some(style) = attr(element, "style") else {
        return false;
    };
    let style: String = style
        .to_ascii_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    style.contains("display:none")
        || style.contains("visibility:hidden")
        || style.contains("mso-hide:all")
        || (style.contains("max-height:0") && style.contains("overflow:hidden"))
        || (style.contains("font-size:0") && style.contains("line-height:0"))
}

/// Elements that start quoted history in the HTML clients mark it explicitly.
fn is_quote_container(element: &Element) -> bool {
    match element.tag {
        "blockquote" => {
            attr(element, "type") == Some("cite")
                || has_class(element, "gmail_quote")
                || has_class(element, "yahoo_quoted")
        }
        "div" => {
            has_class(element, "gmail_quote")
                || has_class(element, "yahoo_quoted")
                || has_class(element, "moz-cite-prefix")
                || matches!(
                    attr(element, "id"),
                    Some(
                        "divRplyFwdMsg"
                            | "appendonsend"
                            | "mail-editor-reference-message-container"
                    )
                )
        }
        _ => false,
    }
}

fn is_signature_container(element: &Element) -> bool {
    has_class(element, "gmail_signature")
        || has_class(element, "moz-signature")
        || matches!(
            attr(element, "id"),
            Some("Signature" | "signature" | "ms-outlook-mobile-signature")
        )
        || attr(element, "data-smartmail") == Some("gmail_signature")
}

fn html_to_markdown(html: &str) -> String {
    let converter = HtmlToMarkdown::builder()
        .skip_tags(vec![
            "head", "style", "script", "noscript", "template", "title", "img", "picture", "svg",
            "video", "audio", "iframe", "object", "map", "form", "button", "input", "select",
        ])
        .add_handler(
            vec![
                "div",
                "span",
                "p",
                "td",
                "tr",
                "table",
                "center",
                "font",
                "blockquote",
                "section",
            ],
            |handlers: &dyn Handlers, element: Element| -> Option<HandlerResult> {
                if is_hidden(&element) {
                    return None;
                }
                if is_quote_container(&element) {
                    let inner = handlers.walk_children(element.node).content;
                    return Some(format!("\n\n{QUOTE_MARK}\n\n{inner}\n\n").into());
                }
                if is_signature_container(&element) {
                    let inner = handlers.walk_children(element.node).content;
                    return Some(format!("\n\n{SIG_MARK}\n\n{inner}\n\n").into());
                }
                if element.tag == "table" {
                    if let Some(table) = data_table(handlers, element.node) {
                        return Some(format!("\n\n{table}\n\n").into());
                    }
                }
                handlers.fallback(element)
            },
        )
        .build();
    let markdown = converter
        .convert(html)
        .unwrap_or_else(|_| TAG.replace_all(html, " ").into_owned());
    // htmd escapes Markdown metacharacters in text ("utm\_source", "\<a@b.c>");
    // a model reads the plain characters better, and URLs need them intact.
    // Table-cell pipes stay escaped.
    let markdown = MD_ESCAPE.replace_all(&markdown, "$1");
    markdown
        .lines()
        // "View this email in your browser" and friends.
        .filter(|line| {
            let visible = MD_LINK.replace_all(line, "$1");
            !(visible.len() < 140 && STUB_PHRASE.is_match(&visible))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

static MD_ESCAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\([\\`*_{}\[\]()#+\-.!<>~=])").expect("escape regex"));

fn tag_name(node: &Node) -> Option<String> {
    match &node.data {
        NodeData::Element { name, .. } => Some(name.local.as_ref().to_ascii_lowercase()),
        _ => None,
    }
}

/// Rows of a table, looking through thead/tbody/tfoot but not into nested tables.
fn table_rows(node: &Rc<Node>, rows: &mut Vec<Rc<Node>>) {
    for child in node.children.borrow().iter() {
        match tag_name(child).as_deref() {
            Some("tr") => rows.push(Rc::clone(child)),
            Some("thead" | "tbody" | "tfoot") => table_rows(child, rows),
            _ => {}
        }
    }
}

fn contains_table(node: &Node) -> bool {
    node.children
        .borrow()
        .iter()
        .any(|child| tag_name(child).as_deref() == Some("table") || contains_table(child))
}

/// A data table (header cells, no nested tables) as a Markdown pipe table.
/// htmd only does this for top-level tables, but marketing and receipt mail
/// nests every data table inside layout tables. Layout tables return `None`
/// and are flattened as usual.
fn data_table(handlers: &dyn Handlers, node: &Rc<Node>) -> Option<String> {
    if contains_table(node) {
        return None;
    }
    let mut row_nodes = Vec::new();
    table_rows(node, &mut row_nodes);
    let mut header_row = false;
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (index, row) in row_nodes.iter().enumerate() {
        let mut cells = Vec::new();
        for cell in row.children.borrow().iter() {
            let tag = tag_name(cell);
            if !matches!(tag.as_deref(), Some("td" | "th")) {
                continue;
            }
            if index == 0 && tag.as_deref() == Some("th") {
                header_row = true;
            }
            let text = handlers.walk_children(cell).content;
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            cells.push(text.replace('|', "\\|"));
        }
        if cells.iter().any(|c| !c.is_empty()) {
            rows.push(cells);
        }
    }
    let width = rows.iter().map(Vec::len).max()?;
    if !header_row || rows.len() < 2 || !(2..=12).contains(&width) {
        return None;
    }
    let line = |cells: &[String]| {
        let mut padded: Vec<&str> = cells.iter().map(String::as_str).collect();
        padded.resize(width, "");
        format!("| {} |", padded.join(" | "))
    };
    let mut out = vec![line(&rows[0]), format!("|{}", " --- |".repeat(width))];
    out.extend(rows[1..].iter().map(|r| line(r)));
    Some(out.join("\n"))
}

static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").expect("tag regex"));

/// Drop zero-width and soft-hyphen padding (marketing preheaders are full of
/// it) and turn non-breaking spaces into spaces.
fn strip_invisible(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}' | '\u{034F}'
            | '\u{00AD}' | '\u{180E}' => None,
            '\u{00A0}' | '\u{2007}' | '\u{202F}' => Some(' '),
            c => Some(c),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Quoted history
// ---------------------------------------------------------------------------

static FORWARD_SUBJECT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(?:fwd?|fw|tr|wg|rv|i|doorst|vb|pd)\s*:").expect("forward subject")
});

/// "<someone> wrote:" and its translations: the whole attribution line, which
/// clients may wrap over two or three lines.
static ATTRIBUTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)^[\s>*_\\]*.{0,300}?\b(?:
            wrote|schrieb|a\ écrit|escribió|ha\ scritto|scrisse|schreef|escreveu|skrev|kirjoitti
          | napisał|napisała|napsal|napsala|a\ scris|írta|написал|написала|написал\(а\)
        )(?:\s.{0,160}?)?\s*:[\s*_]*$",
    )
    .expect("attribution regex")
});

/// Attributions that open with the date ("On …", "Le …", "Am …"): these are
/// unambiguous, while a bare "X wrote:" only counts when a quote follows it.
static ON_ATTRIBUTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[\s>*_\\]*(?:on|am|le|el|il|op|em|den|på|w dniu|dne|pe|v)\s")
        .expect("on-attribution regex")
});

static ORIGINAL_MESSAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)^[\s>*_\\]*-{2,}\s*(?:
            original\ message|reply\ message|ursprüngliche\ nachricht|originalnachricht
          | message\ d'origine|message\ original|mensaje\ original|messaggio\ originale
          | oorspronkelijk\ bericht|mensagem\ original|originalmeddelande|opprinnelig\ melding
          | alkuperäinen\ viesti|wiadomość\ oryginalna|původní\ zpráva
        )\s*-{2,}",
    )
    .expect("original message regex")
});

static FORWARD_MARKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)^[\s>*_\\]*(?:
            -{2,}\s*(?:forwarded\ message|weitergeleitete\ nachricht|message\ transféré|mensaje\ reenviado|messaggio\ inoltrato|doorgestuurd\ bericht|mensagem\ encaminhada)
          | begin\ forwarded\ message
          | anfang\ der\ weitergeleiteten\ nachricht
          | début\ du\ message\ réexpédié
        )",
    )
    .expect("forward marker regex")
});

/// The first line of an Outlook-style header block ("From: …").
static HEADER_FROM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[\s>*_\\]*(?:from|von|de|da|van|från|fra|od|от|lähettäjä|feladó)\s*[*_]*\s*:\s*[*_]*\s*\S")
        .expect("header from regex")
});

/// Other fields of that block.
static HEADER_FIELD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^[\s>*_\\]*(?:sent|date|to|cc|subject|gesendet|datum|an|betreff|envoyé|à|objet|enviado|fecha|para|asunto|inviato|data|a|oggetto|verzonden|aan|onderwerp|skickat|till|ämne|sendt|til|emne|wysłano|do|temat|отправлено|кому|тема)\s*[*_]*\s*:",
    )
    .expect("header field regex")
});

static UNDERSCORE_RULE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:_{8,}|-{8,}|\*\s\*\s\*)\s*$").expect("rule regex"));

fn is_quote_line(line: &str) -> bool {
    line.trim_start().starts_with('>')
}

/// Index of the first line of an Outlook header block starting at `i`.
fn outlook_header_at(lines: &[&str], i: usize) -> bool {
    if !HEADER_FROM.is_match(lines[i]) {
        return false;
    }
    lines
        .iter()
        .skip(i + 1)
        .filter(|l| !l.trim().is_empty())
        .take(5)
        .filter(|l| HEADER_FIELD.is_match(l))
        .count()
        >= 2
}

fn attribution_at(lines: &[&str], i: usize) -> Option<usize> {
    if ATTRIBUTION.is_match(lines[i]) {
        return Some(1);
    }
    // Wrapped over up to three lines ("On Mon, 2 Sep 2026 at 10:15, Alice <\na@b.c> wrote:").
    let mut joined = lines[i].trim_end().to_string();
    for extra in 1..3 {
        let next = lines.get(i + extra)?;
        if next.trim().is_empty() {
            return None;
        }
        joined.push(' ');
        joined.push_str(next.trim());
        if ATTRIBUTION.is_match(&joined) {
            return Some(extra + 1);
        }
    }
    None
}

/// Split the current message from the history it quotes.
fn split_quoted(text: &str) -> (String, Option<String>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut start: Option<usize> = None;
    for i in 0..lines.len() {
        let line = lines[i];
        if line.contains(QUOTE_MARK)
            || ORIGINAL_MESSAGE.is_match(line)
            || outlook_header_at(&lines, i)
            || attribution_at(&lines, i).is_some_and(|span| {
                ON_ATTRIBUTION.is_match(line)
                    || lines
                        .iter()
                        .skip(i + span)
                        .find(|l| !l.trim().is_empty())
                        .is_some_and(|l| is_quote_line(l) || l.contains(QUOTE_MARK))
            })
        {
            start = Some(i);
            break;
        }
    }

    let start = match start {
        Some(i) => Some(i),
        None => trailing_quote_block(&lines),
    };
    let Some(mut start) = start else {
        return (text.to_string(), None);
    };

    // A rule, separator or attribution just above the boundary belongs to it.
    let heads_boundary = |k: usize| {
        UNDERSCORE_RULE.is_match(lines[k])
            || FORWARD_MARKER.is_match(lines[k])
            || ORIGINAL_MESSAGE.is_match(lines[k])
            || attribution_at(&lines, k).is_some()
    };
    while start > 0 {
        let Some(above) = (0..start).rev().find(|&k| !lines[k].trim().is_empty()) else {
            break;
        };
        if !heads_boundary(above) {
            break;
        }
        start = above;
    }

    let history = lines[start..].join("\n");
    // A forward keeps the forwarded message as its content.
    let first_history_line = lines[start..]
        .iter()
        .map(|l| l.replace(QUOTE_MARK, ""))
        .find(|l| !l.trim().is_empty() && !UNDERSCORE_RULE.is_match(l))
        .unwrap_or_default();
    if FORWARD_MARKER.is_match(&first_history_line) {
        return (text.to_string(), None);
    }
    (lines[..start].join("\n"), Some(history))
}

/// A run of `>`-quoted lines that ends the message (top-posted reply). Quotes
/// interleaved with answers are left in place: the answers need them.
fn trailing_quote_block(lines: &[&str]) -> Option<usize> {
    let last_own = lines
        .iter()
        .rposition(|l| !l.trim().is_empty() && !is_quote_line(l))?;
    (last_own + 1..lines.len()).find(|&i| is_quote_line(lines[i]))
}

// ---------------------------------------------------------------------------
// Signatures
// ---------------------------------------------------------------------------

static SENT_FROM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)^[\s*_\\]*(?:
            sent\ from\ (?:my\ |outlook|mail\ for|yahoo\ mail|samsung|proton\ ?mail|aol|gmail)
          | sent\ via\ \S
          | sent\ with\ (?:proton|tutanota)
          | get\ (?:outlook|the\ outlook)\ for
          | envoyé\ (?:de|depuis)\ mon
          | von\ meinem\ .{0,40}gesendet
          | gesendet\ von\ meinem
          | enviado\ (?:desde|do)\ (?:mi|meu)
          | inviato\ da(?:l)?\ (?:mio|iphone|ipad)
          | verzonden\ (?:vanaf|met)\ (?:mijn|iphone)
          | skickat\ från\ min
        ).{0,60}$",
    )
    .expect("sent-from regex")
});

/// Remove the signature from the current message.
fn strip_signature(text: &str) -> (String, bool) {
    if let Some(at) = text.find(SIG_MARK) {
        return (text[..at].to_string(), true);
    }
    let mut lines: Vec<&str> = text.lines().collect();
    let mut stripped = false;

    // RFC 3676 delimiter ("-- "), which Markdown conversion may escape.
    if let Some(at) = lines.iter().rposition(|l| {
        let t = l.trim_end();
        t == "--" || t == "\\--" || t == "\\-\\-"
    }) {
        let remainder = lines[at + 1..]
            .iter()
            .filter(|l| !l.trim().is_empty())
            .count();
        if at > 0 && remainder <= 15 {
            lines.truncate(at);
            stripped = true;
        }
    }

    // Mobile client taglines among the last lines.
    let mut kept_tail = 0;
    let mut i = lines.len();
    while i > 0 && kept_tail < 4 {
        i -= 1;
        if lines[i].trim().is_empty() {
            continue;
        }
        if SENT_FROM.is_match(lines[i]) {
            lines.remove(i);
            stripped = true;
        } else {
            kept_tail += 1;
        }
    }
    (lines.join("\n"), stripped)
}

// ---------------------------------------------------------------------------
// Links and whitespace
// ---------------------------------------------------------------------------

static MD_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\[([^\]\n]*)\]\(\s*<?[^)\s>]*>?(?:\s+"[^"]*")?\s*\)"#).expect("md link")
});

static LINKISH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"!\[[^\]\n]*\]\(\s*<?[^)\s>]*>?(?:\s+"[^"]*")?\s*\)|\[([^\]\n]*)\]\(\s*<?([^)\s>]*)>?(?:\s+"[^"]*")?\s*\)|<(https?://[^>\s]+)>|(https?://[^\s<>()\[\]"']+)"#,
    )
    .expect("linkish regex")
});

const TRACKING_PARAMS: &[&str] = &[
    "fbclid",
    "gclid",
    "dclid",
    "msclkid",
    "yclid",
    "mc_cid",
    "mc_eid",
    "_hsenc",
    "_hsmi",
    "mkt_tok",
    "trk",
    "trkcampaign",
    "ss_source",
    "ss_campaign_id",
    "vero_id",
    "oly_enc_id",
    "oly_anon_id",
    "rb_clickid",
    "s_cid",
    "icid",
    "cmpid",
    "ref_src",
    "sc_cid",
    "wt.mc_id",
];

/// Click-tracking and email-platform redirector domains.
const TRACKING_HOSTS: &[&str] = &[
    "list-manage.com",
    "sendgrid.net",
    "mandrillapp.com",
    "hubspotlinks.com",
    "hs-sites.com",
    "exacttarget.com",
    "rs6.net",
    "createsend.com",
    "createsend1.com",
    "cmail19.com",
    "cmail20.com",
    "mailgun.org",
    "mcsv.net",
    "sparkpostmail.com",
    "mktoedge.com",
    "awstrack.me",
    "pardot.com",
    "mailchimpapp.net",
    "klclick.com",
    "klclick1.com",
    "email-link.net",
];

/// Subdomains email platforms serve their redirectors from.
const TRACKING_SUBDOMAINS: &[&str] = &[
    "click.",
    "clicks.",
    "trk.",
    "track.",
    "tracking.",
    "links.",
    "link.",
    "email.",
    "em.",
    "emails.",
    "mailer.",
    "e.",
];

const TRACKING_PATH_PARTS: &[&str] = &[
    "/track/click",
    "/ls/click",
    "/wf/click",
    "/click?",
    "/ss/c/",
    "/e3t/",
    "/trk/",
    "/redirect?",
    "/r/?",
    "/open.php",
    "/pixel",
];

fn url_host(url: &str) -> &str {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    rest[..end].rsplit('@').next().unwrap_or_default()
}

fn is_tracking(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let host = url_host(&lower);
    TRACKING_HOSTS.iter().any(|domain| {
        host.strip_suffix(domain)
            .is_some_and(|rest| rest.is_empty() || rest.ends_with('.'))
    }) || TRACKING_SUBDOMAINS.iter().any(|sub| host.starts_with(sub))
        || TRACKING_PATH_PARTS.iter().any(|part| lower.contains(part))
}

/// Remove `utm_*` and other click-attribution parameters.
fn strip_tracking_params(url: &str) -> String {
    let (without_fragment, fragment) = match url.split_once('#') {
        Some((u, f)) => (u, Some(f)),
        None => (url, None),
    };
    let Some((base, query)) = without_fragment.split_once('?') else {
        return url.to_string();
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let key = pair
                .split('=')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            !key.is_empty() && !key.starts_with("utm_") && !TRACKING_PARAMS.contains(&key.as_str())
        })
        .collect();
    let mut out = base.to_string();
    if !kept.is_empty() {
        out.push('?');
        out.push_str(&kept.join("&"));
    }
    if let Some(f) = fragment.filter(|f| !f.is_empty()) {
        out.push('#');
        out.push_str(f);
    }
    out
}

/// A URL a reader can use, or `None` when it only carries tracking.
fn usable_url(url: &str) -> Option<String> {
    let url = url.replace("&amp;", "&");
    if !url.starts_with("http://") && !url.starts_with("https://") {
        // mailto:, tel: and relative links stay as they are.
        return Some(url);
    }
    if is_tracking(&url) {
        return None;
    }
    let clean = strip_tracking_params(&url);
    if clean.chars().count() > MAX_URL_CHARS {
        let scheme = if clean.starts_with("https") {
            "https"
        } else {
            "http"
        };
        return Some(format!("{scheme}://{}/…", url_host(&clean)));
    }
    Some(clean)
}

fn clean_links(text: &str) -> String {
    LINKISH
        .replace_all(text, |caps: &Captures| {
            if let Some(url) = caps.get(2) {
                let label = caps.get(1).map_or("", |m| m.as_str()).trim();
                return match usable_url(url.as_str()) {
                    // A link around an image only: nothing left to read.
                    _ if label.is_empty() => String::new(),
                    Some(url) if label == url => url,
                    Some(url) if url.starts_with("mailto:") => label.to_string(),
                    Some(url) => format!("[{label}]({url})"),
                    None => label.to_string(),
                };
            }
            if let Some(url) = caps.get(3).or_else(|| caps.get(4)) {
                // Keep sentence punctuation that the bare-URL match swallowed.
                let raw = url.as_str();
                let trimmed = raw.trim_end_matches(['.', ',', ';', ':', '!', '?']);
                let tail = &raw[trimmed.len()..];
                return match usable_url(trimmed) {
                    Some(url) => format!("{url}{tail}"),
                    None => tail.to_string(),
                };
            }
            // An image.
            String::new()
        })
        .into_owned()
}

static SPACE_RUN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]{2,}").expect("space run"));
static EMPTY_EMPHASIS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\*\*\s*\*\*|__\s*__|\[\s*\]\(\s*\))").expect("empty emphasis")
});

/// Markers out, blank-line runs collapsed, filler lines dropped.
fn tidy(text: &str) -> String {
    let text = text.replace(QUOTE_MARK, "").replace(SIG_MARK, "");
    let mut out: Vec<String> = Vec::new();
    let mut blank = false;
    for line in text.lines() {
        let line = EMPTY_EMPHASIS.replace_all(line.trim_end(), "");
        let table_row = line.trim_start().starts_with('|');
        let line = if table_row {
            line.trim_start().to_string()
        } else {
            // One stray leading space is layout residue; deeper indents nest lists.
            // Count characters, not bytes: indents can hold multi-byte Unicode
            // spaces (U+200A and friends), and slicing them by byte panics.
            let indent = line.chars().take_while(|c| c.is_whitespace()).count();
            let indent = if indent < 2 {
                String::new()
            } else {
                " ".repeat(indent.min(4))
            };
            format!("{indent}{}", SPACE_RUN.replace_all(line.trim_start(), " "))
        };
        let table_rule = table_row && line.contains("---");
        // Lines of pure layout residue: "|", "*", "\-", "* * *" runs and the like.
        let filler = line.chars().all(|c| {
            c.is_whitespace() || matches!(c, '|' | '*' | '_' | '-' | '\\' | '#' | '>' | '=')
        });
        if filler && !table_rule && !UNDERSCORE_RULE.is_match(&line) {
            if !out.is_empty() {
                blank = true;
            }
            continue;
        }
        if blank {
            out.push(String::new());
            blank = false;
        }
        out.push(line);
    }
    out.join("\n").trim().to_string()
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

/// Compute and store the clean body of one cached message body, and refresh
/// the message preview from it.
pub async fn store(
    pool: &SqlitePool,
    message_id: &str,
    text: Option<&str>,
    html: Option<&str>,
) -> sqlx::Result<CleanBody> {
    let subject: Option<String> = sqlx::query_scalar("SELECT subject FROM messages WHERE id = ?")
        .bind(message_id)
        .fetch_optional(pool)
        .await?
        .flatten();
    // One malformed message must not take down the task cleaning it (the startup
    // backfill, a sync prefetch or a read): fall back to its text part, mark it
    // cleaned under the current rules so the backfill moves on, and log it.
    let cleaned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        clean(text, html, subject.as_deref())
    }))
    .unwrap_or_else(|_| {
        tracing::warn!("cleaning message {message_id} panicked; stored its text part instead");
        CleanBody {
            body: text.unwrap_or_default().trim().to_string(),
            quoted_history: None,
            signature_stripped: false,
        }
    });
    // The clean body, its search-index entry and the preview change
    // together.
    let mut tx = crate::db::begin_write(pool).await?;
    sqlx::query(
        r#"UPDATE message_bodies
           SET clean_body = ?, quoted_history = ?, signature_stripped = ?, clean_version = ?
           WHERE message_id = ?"#,
    )
    .bind(&cleaned.body)
    .bind(&cleaned.quoted_history)
    .bind(cleaned.signature_stripped)
    .bind(CLEAN_VERSION)
    .bind(message_id)
    .execute(&mut *tx)
    .await?;
    crate::search_index::index_body(
        &mut tx,
        message_id,
        &cleaned.body,
        cleaned.quoted_history.as_deref(),
    )
    .await?;
    if let Some(preview) = preview(&cleaned.body) {
        sqlx::query("UPDATE messages SET preview = ? WHERE id = ? AND preview IS NOT ?")
            .bind(&preview)
            .bind(message_id)
            .bind(&preview)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(cleaned)
}

#[derive(sqlx::FromRow)]
struct PendingBody {
    message_id: String,
    text_body: Option<String>,
    html_body: Option<String>,
}

/// Clean every cached body stored before this version of the rules, in small
/// batches so sync and API writers keep getting the write lock.
pub async fn backfill(pool: &SqlitePool) -> sqlx::Result<usize> {
    let mut done = 0;
    loop {
        let batch: Vec<PendingBody> = sqlx::query_as(
            r#"SELECT message_id, text_body, html_body FROM message_bodies
               WHERE clean_version < ? LIMIT 50"#,
        )
        .bind(CLEAN_VERSION)
        .fetch_all(pool)
        .await?;
        if batch.is_empty() {
            return Ok(done);
        }
        for row in batch {
            store(
                pool,
                &row.message_id,
                row.text_body.as_deref(),
                row.html_body.as_deref(),
            )
            .await?;
            done += 1;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// Run [`backfill`] in the background after startup.
pub fn spawn_backfill(pool: SqlitePool) {
    tokio::spawn(async move {
        match backfill(&pool).await {
            Ok(0) => {}
            Ok(n) => tracing::info!("cleaned {n} cached message bodies"),
            Err(e) => tracing::warn!("clean-body backfill stopped: {e}"),
        }
    });
}

#[cfg(test)]
mod tests;
