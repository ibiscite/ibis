//! The old loader's bounded Zotero tag allowlist, preserving escaped entities.
pub(super) fn normalize(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut rest = title;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some((tag, len)) = rich_text_tag(rest)
        {
            out.push_str(tag);
            rest = &rest[len..];
            continue;
        }
        if c == '&'
            && let Some(len) = entity_len(rest)
        {
            out.push_str(&rest[..len]);
            rest = &rest[len..];
            continue;
        }
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
        rest = &rest[c.len_utf8()..];
    }
    out
}

fn rich_text_tag(rest: &str) -> Option<(&'static str, usize)> {
    // A comparison such as `a < b > c` must stay literal. Bound the scan so
    // long runs of `<` or spaces take linear work, as in the old loader.
    if rest.as_bytes().get(1).is_some_and(u8::is_ascii_whitespace) {
        return None;
    }
    let end = rest.bytes().take(64).position(|b| b == b'>')?;
    let inner = rest[1..end].trim_matches(|c: char| c.is_ascii_whitespace());
    let (closing, inner) = match inner.strip_prefix('/') {
        Some(inner) => (true, inner.trim_start_matches(|c: char| c.is_ascii_whitespace())),
        None => (false, inner),
    };
    let mut words = inner.split_ascii_whitespace();
    let name = words.next()?.to_ascii_lowercase();
    let attribute = words.next().map(str::to_ascii_lowercase);
    if words.next().is_some() {
        return None;
    }
    let (open, close) = match (name.as_str(), attribute.as_deref()) {
        ("i", None) => ("<i>", "</i>"),
        ("b", None) => ("<b>", "</b>"),
        ("sub", None) => ("<sub>", "</sub>"),
        ("sup", None) => ("<sup>", "</sup>"),
        ("span", None) if closing => ("", "</span>"),
        ("span", Some("class=\"nocase\"")) => ("<span class=\"nocase\">", "</span>"),
        ("span", Some("style=\"font-variant:small-caps;\"")) => {
            ("<span style=\"font-variant:small-caps;\">", "</span>")
        }
        _ => return None,
    };
    let tag = if closing { close } else { open };
    (!tag.is_empty()).then_some((tag, end + 1))
}

// Preserve CSL's named and numeric entities exactly once. The bounded scan
// also keeps a long sequence of unterminated ampersands linear.
fn entity_len(rest: &str) -> Option<usize> {
    let end = rest.bytes().take(12).position(|byte| byte == b';')?;
    let entity = &rest[1..end];
    let valid = match entity {
        "amp" | "lt" | "gt" | "quot" | "apos" | "nbsp" => true,
        value if value.starts_with("#x") => u32::from_str_radix(&value[2..], 16)
            .ok()
            .and_then(char::from_u32)
            .is_some(),
        value if value.starts_with('#') => value[1..].parse::<u32>().ok().and_then(char::from_u32).is_some(),
        _ => false,
    };
    valid.then_some(end + 1)
}
