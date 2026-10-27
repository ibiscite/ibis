//! One public URL boundary for every source adapter.
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) fn remove_private_urls(csl: &mut BTreeMap<String, Value>) -> bool {
    let mut removed = csl
        .get("URL")
        .and_then(Value::as_str)
        .is_some_and(|url| !public_url(url));
    if removed {
        csl.remove("URL");
    }
    if let Some(note) = csl.get("note").and_then(Value::as_str) {
        let (safe, note_removed) = without_private_urls(note);
        if note_removed {
            csl.insert("note".into(), Value::String(safe));
            removed = true;
        }
    }
    removed
}

fn public_url(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty() || url.contains('\\') || url.chars().any(char::is_control) {
        return false;
    }
    let Some((scheme, rest)) = url.split_once(':') else {
        // Zotero accepts a domain without a scheme. Require a domain-shaped
        // host so relative paths with separators remain engine-only.
        let host = url.split('/').next().unwrap_or_default();
        let labels: Vec<_> = host.split('.').collect();
        let document_suffix = labels.last().is_some_and(|label| {
            matches!(
                label.to_ascii_lowercase().as_str(),
                "pdf"
                    | "epub"
                    | "djvu"
                    | "doc"
                    | "docx"
                    | "odt"
                    | "rtf"
                    | "txt"
                    | "md"
                    | "tex"
                    | "bib"
                    | "ris"
                    | "json"
                    | "sqlite"
                    | "html"
                    | "htm"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "svg"
                    | "webp"
                    | "mp3"
                    | "mp4"
                    | "zip"
            )
        });
        return !document_suffix
            && labels.len() >= 2
            && labels
                .iter()
                .all(|label| !label.is_empty() && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
            && labels
                .last()
                .is_some_and(|label| label.len() >= 2 && label.bytes().all(|b| b.is_ascii_alphabetic()))
            && !url.chars().any(char::is_whitespace);
    };
    scheme.len() > 1
        && !["file", "smb", "afp", "nfs", "jar"]
            .iter()
            .any(|private| scheme.eq_ignore_ascii_case(private))
        && !rest.is_empty()
        && scheme
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphabetic() || (i > 0 && (b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.'))))
}

fn without_private_urls(note: &str) -> (String, bool) {
    let mut removed = false;
    let mut safe = String::with_capacity(note.len());
    let mut cursor = 0;
    let mut line_start = true;
    while cursor < note.len() {
        let rest = &note[cursor..];
        let inline = rest.starts_with("{:");
        let label_start = if inline { 2 } else { 0 };
        if (inline || line_start)
            && rest
                .get(label_start..label_start + 4)
                .is_some_and(|label| label.eq_ignore_ascii_case("URL:"))
        {
            let start = label_start + 4;
            let value_start = start + rest[start..].len() - rest[start..].trim_start().len();
            if rest[start..value_start].contains(['\r', '\n', '\u{2028}', '\u{2029}'])
                && rest
                    .get(value_start..value_start + 4)
                    .is_some_and(|label| label.eq_ignore_ascii_case("URL:"))
            {
                // A blank URL line cannot swallow the next URL field as if
                // its label were a URI scheme. Recheck that field separately.
                safe.push_str(&rest[..value_start]);
                cursor += value_start;
                line_start = true;
                continue;
            }
            // citeproc reads the value through any `{` up to the closing
            // brace or the end of the line, so judge exactly that text.
            let value_end = value_start
                + rest[value_start..]
                    .find(['}', '\r', '\n', '\u{2028}', '\u{2029}'])
                    .unwrap_or(rest.len() - value_start);
            let value = rest[value_start..value_end].trim();
            if !value.is_empty() && (value.contains('{') || !public_url(value)) {
                removed = true;
                // Braces and lone CR can be part of a private path. Remove
                // through the actual line separator, including inline fields.
                let end = value_start
                    + rest[value_start..]
                        .find(['\n', '\u{2028}', '\u{2029}'])
                        .unwrap_or(rest.len() - value_start);
                // Keep line boundaries: deleting a field cannot turn following
                // prose into the first line accepted by citeproc's field hacks.
                for ch in rest[..end]
                    .chars()
                    .filter(|c| matches!(c, '\r' | '\n' | '\u{2028}' | '\u{2029}'))
                {
                    safe.push(ch);
                }
                line_start = inline
                    || rest[..end]
                        .chars()
                        .next_back()
                        .is_some_and(|c| matches!(c, '\r' | '\n' | '\u{2028}' | '\u{2029}'));
                cursor += end;
                continue;
            }
            if !value.is_empty() {
                // Consume each segment once, including adversarial repeated
                // inline fields with no closing brace. Nested fields start a
                // new segment and receive the same URL check.
                safe.push_str(&rest[..value_end]);
                cursor += value_end;
                line_start = false;
                continue;
            }
        }
        if let Some(ch) = rest.chars().next() {
            safe.push(ch);
            cursor += ch.len_utf8();
            line_start = matches!(ch, '}' | '\r' | '\n' | '\u{2028}' | '\u{2029}');
        }
    }
    (safe, removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_urls_and_their_note_overrides_are_preserved() {
        for url in [
            "https://example.invalid/work",
            "example.invalid/work",
            "doi:10.1000/invented",
        ] {
            let mut fields = BTreeMap::from([
                ("URL".into(), Value::String(url.into())),
                ("note".into(), Value::String(format!("URL: {url}\n{{:URL: {url}}}"))),
            ]);
            let expected = fields.clone();
            assert!(!remove_private_urls(&mut fields));
            assert_eq!(fields, expected);
        }
    }

    #[test]
    fn a_brace_cannot_hide_a_private_note_url() {
        for note in [
            "URL: {/home/invented/private.pdf",
            "{:URL: {file:///home/invented/private.pdf}",
            "URL: example.invalid{/home/invented/private.pdf",
            "Prose\nURL: {C:\\invented\\private.pdf",
        ] {
            let mut fields = BTreeMap::from([("note".into(), Value::String(note.into()))]);
            assert!(remove_private_urls(&mut fields), "{note}");
            let kept = fields.get("note").and_then(Value::as_str).unwrap_or_default();
            assert!(!kept.contains("invented"), "{note} kept {kept}");
        }
    }
}
