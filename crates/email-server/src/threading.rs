use std::collections::HashSet;

pub fn message_ids(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .filter_map(|token| {
            let token = token.trim_matches(|c: char| c == '<' || c == '>');
            (!token.is_empty() && token.contains('@')).then(|| token.to_ascii_lowercase())
        })
        .collect()
}

pub fn header_value(raw_headers: &str, name: &str) -> Option<String> {
    let mut value = String::new();
    let mut matched = false;
    for line in raw_headers.lines() {
        if line.starts_with([' ', '\t']) {
            if matched {
                value.push(' ');
                value.push_str(line.trim());
            }
            continue;
        }
        if matched {
            break;
        }
        let (key, rest) = line.split_once(':')?;
        matched = key.eq_ignore_ascii_case(name);
        if matched {
            value.push_str(rest.trim());
        }
    }
    (!value.is_empty()).then_some(value)
}

pub fn references(raw_headers: &str) -> Vec<String> {
    let mut ids = header_value(raw_headers, "References")
        .map(|value| message_ids(&value))
        .unwrap_or_default();
    if let Some(value) = header_value(raw_headers, "In-Reply-To") {
        ids.extend(message_ids(&value));
    }
    let mut seen = HashSet::new();
    ids.retain(|id| seen.insert(id.clone()));
    ids
}

pub fn normalized_subject(subject: Option<&str>) -> Option<String> {
    let mut subject = subject?.trim().to_ascii_lowercase();
    loop {
        let stripped = subject
            .strip_prefix("re:")
            .or_else(|| subject.strip_prefix("fwd:"))
            .map(str::trim);
        let Some(stripped) = stripped else { break };
        if stripped == subject {
            break;
        }
        subject = stripped.to_string();
    }
    (!subject.is_empty()).then_some(subject)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_folded_references_and_reply_to() {
        let raw = "References: <root@example.test>\r\n <middle@example.test>\r\nIn-Reply-To: <middle@example.test>\r\n";
        assert_eq!(
            references(raw),
            ["root@example.test", "middle@example.test"]
        );
    }

    #[test]
    fn strips_reply_prefixes_from_subjects() {
        assert_eq!(
            normalized_subject(Some("Re: Fwd: Project")),
            Some("project".into())
        );
    }
}
