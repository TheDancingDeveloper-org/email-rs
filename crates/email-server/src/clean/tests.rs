use super::*;

fn parse(raw: &str) -> (Option<String>, Option<String>, Option<String>) {
    let parsed = mail_parser::MessageParser::default()
        .parse(raw.as_bytes())
        .expect("fixture parses");
    let subject = parsed.subject().map(ToString::to_string);
    let body = crate::providers::parse_message_body(1, raw.as_bytes()).expect("fixture body");
    (body.text_body, body.html_body, subject)
}

fn clean_fixture(raw: &str) -> CleanBody {
    let (text, html, subject) = parse(raw);
    clean(text.as_deref(), html.as_deref(), subject.as_deref())
}

const GMAIL: &str = include_str!("fixtures/gmail_reply_chain.eml");
const OUTLOOK: &str = include_str!("fixtures/outlook_reply.eml");
const MARKETING: &str = include_str!("fixtures/marketing_html_only.eml");
const PLAIN: &str = include_str!("fixtures/plain_signature.eml");

#[test]
fn gmail_reply_chain_keeps_only_the_new_message() {
    let cleaned = clean_fixture(GMAIL);
    assert_eq!(
        cleaned.body,
        "Hi Tom,\n\nThe Leeds venue works for us. Can you confirm the booking by Friday 2 October? They need a deposit of £500.\n\nThanks,\nPriya"
    );
    assert!(cleaned.signature_stripped);
    let history = cleaned.quoted_history.expect("history split off");
    assert!(history.starts_with("On Mon, 28 Sep 2026 at 17:02, Tom Baker"));
    assert!(history.contains("Which do you prefer?"));
}

#[test]
fn gmail_html_part_splits_on_the_quote_container() {
    let (_, html, subject) = parse(GMAIL);
    let cleaned = clean(None, html.as_deref(), subject.as_deref());
    assert!(cleaned.body.starts_with("Hi Tom,"), "{}", cleaned.body);
    assert!(cleaned.body.contains("deposit of £500."));
    assert!(
        !cleaned.body.contains("Head of Operations"),
        "{}",
        cleaned.body
    );
    assert!(!cleaned.body.contains("wrote:"), "{}", cleaned.body);
    assert!(!cleaned.body.contains("Which do you prefer"));
    assert!(cleaned.signature_stripped);
    let history = cleaned.quoted_history.expect("history");
    assert!(
        history.contains("On Mon, 28 Sep 2026 at 17:02"),
        "{history}"
    );
    assert!(history.contains("Which do you prefer?"));
}

#[test]
fn outlook_reply_drops_the_header_block_and_mobile_tagline() {
    let cleaned = clean_fixture(OUTLOOK);
    assert!(cleaned.body.starts_with("Hi Tom,"), "{}", cleaned.body);
    assert!(cleaned.body.contains("within 3 working days."));
    assert!(!cleaned.body.contains("Get Outlook"), "{}", cleaned.body);
    assert!(
        !cleaned.body.contains("14 days overdue"),
        "{}",
        cleaned.body
    );
    assert!(!cleaned.body.contains("________"), "{}", cleaned.body);
    assert!(cleaned.signature_stripped);
    let history = cleaned.quoted_history.expect("history");
    assert!(history.contains("From: Tom Baker"), "{history}");
    assert!(history.contains("14 days overdue"));
}

#[test]
fn outlook_html_part_splits_on_the_reply_header() {
    let (_, html, subject) = parse(OUTLOOK);
    let cleaned = clean(None, html.as_deref(), subject.as_deref());
    assert!(cleaned.body.starts_with("Hi Tom,"), "{}", cleaned.body);
    assert!(cleaned.body.contains("within 3 working days."));
    assert!(!cleaned.body.contains("margin-top"), "{}", cleaned.body);
    assert!(
        !cleaned.body.contains("Accounts Payable"),
        "{}",
        cleaned.body
    );
    assert!(
        !cleaned.body.contains("Outlook for iOS"),
        "{}",
        cleaned.body
    );
    assert!(!cleaned.body.contains("overdue"), "{}", cleaned.body);
    let history = cleaned.quoted_history.expect("history");
    assert!(history.contains("14 days overdue"), "{history}");
}

#[test]
fn marketing_html_loses_styles_pixels_and_tracking_but_keeps_content() {
    let (text, html, _) = parse(MARKETING);
    let cleaned = clean_fixture(MARKETING);
    let body = &cleaned.body;
    assert!(
        body.contains("# Autumn sale: 30% off waterproofs"),
        "{body}"
    );
    assert!(body.contains("Every waterproof jacket and overtrouser is 30% off"));
    // Header-bearing table survives as a Markdown table.
    assert!(
        body.contains(
            "| Product | Was | Now |\n| --- | --- | --- |\n| Ridge Shell Jacket | £180 | £126 |"
        ),
        "{body}"
    );
    // The "view in browser" line goes; the footer stays.
    assert!(!body.contains("in your browser"), "{body}");
    assert!(
        body.contains("Unsubscribe | Trailhead Outdoor Ltd"),
        "{body}"
    );
    // Clean link kept, utm_* removed, other parameters kept.
    assert!(
        body.contains(
            "[layering guide](https://www.trailhead-outdoor.com/guides/layering?ref=autumn)"
        ),
        "{body}"
    );
    // Tracked links keep their label only.
    assert!(body.contains("Shop the sale"));
    assert!(!body.contains("click.mail"), "{body}");
    assert!(!body.contains("list-manage"), "{body}");
    assert!(!body.contains("utm_"), "{body}");
    // No CSS, images, pixels or hidden preheader padding.
    assert!(!body.contains("webkit"), "{body}");
    assert!(!body.contains("logo.png"), "{body}");
    assert!(!body.contains("open.php"), "{body}");
    assert!(!body.contains("Rain-ready kit"), "{body}");
    assert!(!body.contains('\u{034F}') && !body.contains('\u{200C}'));
    assert!(cleaned.quoted_history.is_none());
    // A fraction of the raw HTML.
    let raw = html.expect("html").len();
    assert!(body.len() * 4 < raw, "{} vs {}", body.len(), raw);
    // mail-parser's synthesized text part was not preferred over the HTML.
    assert!(text.is_some());
}

#[test]
fn plain_text_signature_and_mobile_tagline_are_dropped() {
    let cleaned = clean_fixture(PLAIN);
    assert_eq!(
        cleaned.body,
        "Hey Tom,\n\nCan I borrow your bike rack on Saturday? I'll pick it up around 9am\nand have it back by Sunday evening.\n\nAlso, the club ride has moved to Glenridding - details at\nhttps://www.example-cycling.club/rides/2026-10-03?ride=42\n\nCheers,\nDave"
    );
    assert!(cleaned.signature_stripped);
    assert!(cleaned.quoted_history.is_none());
}

#[test]
fn stub_text_part_falls_back_to_html() {
    let cleaned = clean(
        Some("View this email in your browser: https://example.com/v/123"),
        Some("<html><body><p>Your order <b>#4411</b> has shipped.</p></body></html>"),
        Some("Your order has shipped"),
    );
    assert_eq!(cleaned.body, "Your order **#4411** has shipped.");
}

#[test]
fn empty_text_part_falls_back_to_html() {
    let cleaned = clean(Some("  \r\n"), Some("<p>Meeting moved to 3pm.</p>"), None);
    assert_eq!(cleaned.body, "Meeting moved to 3pm.");
}

#[test]
fn localized_attributions_split() {
    for (attribution, lang) in [
        (
            "Am 28.09.2026 um 17:02 schrieb Tom Baker <tom@example.org>:",
            "de",
        ),
        (
            "Le lun. 28 sept. 2026 à 17:02, Tom Baker <tom@example.org> a écrit :",
            "fr",
        ),
        (
            "El lun, 28 sept 2026 a las 17:02, Tom Baker (<tom@example.org>) escribió:",
            "es",
        ),
        (
            "Il giorno lun 28 set 2026 alle ore 17:02 Tom Baker <tom@example.org> ha scritto:",
            "it",
        ),
        (
            "Op ma 28 sep. 2026 om 17:02 schreef Tom Baker <tom@example.org>:",
            "nl",
        ),
        (
            "On 28 Sep 2026, at 17:02, Tom Baker <tom@example.org> wrote:",
            "apple",
        ),
    ] {
        let text =
            format!("Sounds good, see you then.\n\n{attribution}\n\n> Shall we meet at 10?\n");
        let cleaned = clean(Some(&text), None, None);
        assert_eq!(cleaned.body, "Sounds good, see you then.", "{lang}");
        assert!(
            cleaned
                .quoted_history
                .as_deref()
                .unwrap_or_default()
                .contains("meet at 10"),
            "{lang}"
        );
    }
}

#[test]
fn wrapped_gmail_attribution_splits() {
    let text = "Yes, Thursday works.\n\nOn Mon, 28 Sep 2026 at 17:02, Tom Baker <\ntom@example.org> wrote:\n\n> Does Thursday work?\n";
    let cleaned = clean(Some(text), None, None);
    assert_eq!(cleaned.body, "Yes, Thursday works.");
}

#[test]
fn original_message_separator_splits() {
    let text = "Approved.\n\n-----Original Message-----\nFrom: Tom Baker\nSent: Monday\nSubject: Leave request\n\nCan I take Friday off?";
    let cleaned = clean(Some(text), None, None);
    assert_eq!(cleaned.body, "Approved.");
    assert!(cleaned.quoted_history.unwrap().contains("Friday off"));
}

#[test]
fn german_outlook_header_block_splits() {
    let text = "Danke, erledigt.\n\nVon: Tom Baker <tom@example.org>\nGesendet: Montag, 28. September 2026 17:02\nAn: Anna Weber <anna@example.de>\nBetreff: Rechnung\n\nBitte bis Freitag bezahlen.";
    let cleaned = clean(Some(text), None, None);
    assert_eq!(cleaned.body, "Danke, erledigt.");
}

#[test]
fn trailing_quote_block_splits_but_inline_answers_stay() {
    let top_posted = "Fine by me.\n\n> Can we move the call to 4pm?\n> Thanks\n";
    let cleaned = clean(Some(top_posted), None, None);
    assert_eq!(cleaned.body, "Fine by me.");
    assert!(cleaned.quoted_history.is_some());

    let inline = "> Can we move the call to 4pm?\nYes.\n\n> And invite Sam?\nAlready done.";
    let cleaned = clean(Some(inline), None, None);
    assert!(cleaned.body.contains("> Can we move the call to 4pm?"));
    assert!(cleaned.body.contains("Already done."));
    assert!(cleaned.quoted_history.is_none());
}

#[test]
fn forwards_keep_the_forwarded_message() {
    let text = "FYI, see below.\n\n---------- Forwarded message ---------\nFrom: Bank <alerts@bank.example>\nDate: Mon, 28 Sep 2026\nSubject: Payment due\nTo: <tom@example.org>\n\nYour payment of £120 is due on 5 October.";
    let cleaned = clean(Some(text), None, Some("Fwd: Payment due"));
    assert!(
        cleaned.body.contains("due on 5 October"),
        "{}",
        cleaned.body
    );
    assert!(cleaned.quoted_history.is_none());

    // Recognised from the marker even without the subject prefix.
    let cleaned = clean(Some(text), None, Some("Payment due"));
    assert!(
        cleaned.body.contains("due on 5 October"),
        "{}",
        cleaned.body
    );
}

#[test]
fn quote_only_message_keeps_the_history_as_body() {
    let text = "On Mon, 28 Sep 2026 at 17:02, Tom Baker <tom@example.org> wrote:\n\n> The report is attached.\n";
    let cleaned = clean(Some(text), None, None);
    assert!(cleaned.body.contains("The report is attached."));
    assert!(cleaned.quoted_history.is_none());
}

#[test]
fn a_sentence_ending_in_wrote_is_not_a_quote_header() {
    let text = "Here is what the auditor wrote:\nthe figures reconcile.\n\nRegards";
    let cleaned = clean(Some(text), None, None);
    assert_eq!(cleaned.body, text);
    assert!(cleaned.quoted_history.is_none());
}

#[test]
fn urls_lose_tracking_parameters_and_long_ones_shrink() {
    let text = "Docs: https://docs.example.com/a?utm_source=x&utm_medium=y&id=7&fbclid=abc.\nTrack: https://example.com/very/long/path/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa?x=1\nMail me: [Tom](mailto:tom@example.org)";
    let cleaned = clean(Some(text), None, None);
    assert_eq!(
        cleaned.body,
        "Docs: https://docs.example.com/a?id=7.\nTrack: https://example.com/…\nMail me: Tom"
    );
}

#[test]
fn preview_is_one_flat_line() {
    assert_eq!(
        preview("# Sale\n\nSee [the guide](https://x.example/g) now.").as_deref(),
        Some("Sale See the guide now.")
    );
    assert_eq!(preview("   "), None);
}

#[test]
fn multibyte_space_indents_do_not_panic() {
    // Prod panic (2026-09-30): an indent of U+200A HAIR SPACE (3 bytes) was sliced
    // by byte count and split the character.
    let text = "Hello\n \u{200a}\u{200a}indented by hair spaces\n\u{2003}\u{2003}\u{2003}em spaces\n\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}nbsp run\n\u{3000}\u{3000}ideographic\nbye";
    let cleaned = clean(Some(text), None, None);
    for words in [
        "indented by hair spaces",
        "em spaces",
        "nbsp run",
        "ideographic",
        "bye",
    ] {
        assert!(
            cleaned.body.contains(words),
            "{words:?} missing from {:?}",
            cleaned.body
        );
    }
    // Deeper indents are normalised to at most four ASCII spaces.
    assert!(cleaned
        .body
        .lines()
        .all(|l| !l.starts_with('\u{200a}') && !l.starts_with("     ")));
}
