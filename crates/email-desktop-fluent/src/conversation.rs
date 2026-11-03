use crate::prelude::*;
use crate::*;

/// The message body shown before historical quoted text.  Server-normalised
/// fields are preferred, with a conservative client fallback for older
/// messages that predate `clean_body`/`quoted_history`.
pub(crate) fn conversation_body(body: &MessageBody) -> (&str, Option<&str>) {
    if let Some(clean) = body
        .clean_body
        .as_deref()
        .filter(|text| !text.trim().is_empty())
    {
        return (clean, body.quoted_history.as_deref());
    }
    let text = body.text_body.as_deref().unwrap_or_default();
    split_quoted_text(text)
}

/// Split the common plain-text reply markers without treating an indented
/// paragraph as a quote.  This keeps folding useful for old cached messages.
pub(crate) fn split_quoted_text(text: &str) -> (&str, Option<&str>) {
    let start = text
        .lines()
        .enumerate()
        .find(|(_, line)| {
            let trimmed = line.trim_start();
            trimmed.starts_with('>') || trimmed.eq_ignore_ascii_case("on ... wrote:")
        })
        .map(|(index, _)| {
            text.lines()
                .take(index)
                .map(|line| line.len() + 1)
                .sum::<usize>()
        });
    match start {
        Some(index) if index > 0 => (&text[..index - 1], Some(&text[index..])),
        _ => (text, None),
    }
}

pub(crate) fn quote_toggle_label(expanded: bool) -> &'static str {
    if expanded {
        "Hide quoted text"
    } else {
        "Show quoted text"
    }
}

pub(crate) fn toggle_quoted_text(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        state.quoted_text_expanded = !state.quoted_text_expanded;
        cx.notify();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_quote_markers_and_preserves_body() {
        let (body, quote) = split_quoted_text("Reply\n\n> Previous\n> line");
        assert_eq!(body, "Reply\n");
        assert_eq!(quote, Some("> Previous\n> line"));
    }

    #[test]
    fn does_not_fold_indented_prose() {
        assert_eq!(
            split_quoted_text("Reply\n  indented prose"),
            ("Reply\n  indented prose", None)
        );
    }
}
