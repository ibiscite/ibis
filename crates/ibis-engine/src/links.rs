//! Untrusted launch links can supply search text, never delivery authority.

/// Exactly one nonblank query, using form URL encoding (`+` means space).
/// Match the macOS Search service's 200-character bound; never truncate input.
pub fn parse(link: &str) -> Option<String> {
    let encoded = link.strip_prefix("ibis://search?q=")?;
    // At most four UTF-8 bytes, each percent-encoded, per character.
    if encoded.len() > 200 * 4 * 3 || encoded.contains(['&', '#']) {
        return None;
    }
    let mut decoded = Vec::with_capacity(encoded.len());
    let mut bytes = encoded.bytes();
    while let Some(byte) = bytes.next() {
        decoded.push(match byte {
            b'%' => {
                let high = char::from(bytes.next()?).to_digit(16)?;
                let low = char::from(bytes.next()?).to_digit(16)?;
                (high * 16 + low) as u8
            }
            b'+' => b' ',
            byte => byte,
        });
    }
    let query = String::from_utf8(decoded).ok()?;
    if query.trim().is_empty()
        || query.chars().count() > 200
        || query
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
    {
        return None;
    }
    Some(query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_decode_unicode_spaces_and_punctuation_once() {
        assert_eq!(
            parse("ibis://search?q=%E9%9B%AA+%26+caf%C3%A9%3F%20%2B%20%2520"),
            Some("雪 & café? + %20".into())
        );
    }

    #[test]
    fn links_refuse_malformed_encoding_controls_and_non_queries() {
        for suffix in [
            "%",
            "%2",
            "%GG",
            "%FF",
            "%C0%AF",
            "%ED%A0%80",
            "%E2%82",
            "%00",
            "%09",
            "%0A",
            "%0D",
            "%7F",
            "%C2%85",
            "%E2%80%A8",
            "%E2%80%A9",
            "\nword",
            "word\t",
            "",
            "+%20",
        ] {
            assert!(parse(&format!("ibis://search?q={suffix}")).is_none(), "{suffix:?}");
        }
    }

    #[test]
    fn links_refuse_every_action_or_option_that_could_do_more_than_search() {
        for link in [
            "https://search?q=word",
            "ibis:search?q=word",
            "ibis://paste?q=word",
            "ibis://search",
            "ibis://search?query=word",
            "ibis://search?%71=word",
            "ibis://search/?q=word",
            "ibis://search/path?q=word",
            "ibis://user@search?q=word",
            "ibis://search:123?q=word",
            "ibis://search?q=word#",
            "ibis://search?q=first&q=last",
            "ibis://search?q=word&%71=last",
            "ibis://search?q=word&",
            "ibis://search?q=word&paste=true",
            "ibis://search?q=word&path=/invented/library.bib",
            "ibis://search?q=word&library=/invented/library.bib",
            "ibis://search?q=word&output=latex",
        ] {
            assert!(parse(link).is_none(), "{link}");
        }
    }

    #[test]
    fn links_use_the_existing_services_limit_of_200_unicode_characters() {
        let limit = "雪".repeat(200);
        assert_eq!(parse(&format!("ibis://search?q={limit}")), Some(limit));
        assert!(parse(&format!("ibis://search?q={}", "雪".repeat(201))).is_none());
        assert!(parse(&format!("ibis://search?q={}", "a".repeat(201))).is_none());
    }
}
