//! Who made a style and on what terms, read from its `<info>`. The bundle keeps
//! every file byte for byte; this only reads it for the credits view.
use crate::catalog::Person;

/// The parts of `<info>` a credit shows.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Info {
    pub summary: Option<String>,
    pub authors: Vec<Person>,
    pub contributors: Vec<Person>,
    pub rights: Option<String>,
    pub license: Option<String>,
    pub documentation: Vec<String>,
}

impl Info {
    /// Missing or malformed parts are left out, never guessed.
    pub(crate) fn read(xml: &str) -> Self {
        let xml = without_comments(xml);
        let Some(info) = inner(&xml, "info").into_iter().next() else {
            return Self::default();
        };
        let people = |role: &str| {
            inner(info, role)
                .into_iter()
                .filter_map(|person| {
                    Some(Person {
                        name: text(inner(person, "name").first()?)?,
                        uri: inner(person, "uri").first().and_then(|uri| web_address(uri)),
                    })
                })
                .collect()
        };
        let rights = tags(info, "rights").into_iter().next();
        Self {
            summary: inner(info, "summary").first().and_then(|summary| text(summary)),
            authors: people("author"),
            contributors: people("contributor"),
            rights: rights.and_then(|(_, content)| text(content)),
            license: rights
                .and_then(|(tag, _)| attribute(tag, "license"))
                .and_then(|uri| web_address(&uri)),
            documentation: tags(info, "link")
                .into_iter()
                .filter(|(tag, _)| attribute(tag, "rel").as_deref() == Some("documentation"))
                .filter_map(|(tag, _)| attribute(tag, "href").and_then(|uri| web_address(&uri)))
                .collect(),
        }
    }
}

fn without_comments(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        rest = rest[start + 4..].split_once("-->").map_or("", |(_, after)| after);
    }
    out.push_str(rest);
    out
}

/// Each `<name …>content</name>` directly in `xml`, as (opening tag, content);
/// a self-closing tag has empty content.
fn tags<'a>(xml: &'a str, name: &str) -> Vec<(&'a str, &'a str)> {
    let mut found = Vec::new();
    let mut rest = xml;
    let open = format!("<{name}");
    let close = format!("</{name}>");
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        // `<link` must not match `<links`.
        if !after.starts_with(|ch: char| ch == '>' || ch == '/' || ch.is_whitespace()) {
            rest = after;
            continue;
        }
        let Some(end) = after.find('>') else { break };
        let tag = &after[..end];
        rest = &after[end + 1..];
        if tag.ends_with('/') {
            found.push((tag, ""));
            continue;
        }
        let Some(stop) = rest.find(&close) else { break };
        found.push((tag, &rest[..stop]));
        rest = &rest[stop + close.len()..];
    }
    found
}

fn inner<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    tags(xml, name).into_iter().map(|(_, content)| content).collect()
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    while let Some(at) = rest.find(name) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + name.len()..].trim_start();
        rest = &rest[at + name.len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else { continue };
        let value = value.trim_start();
        let quote = value.chars().next().filter(|ch| matches!(ch, '"' | '\''))?;
        let value = &value[1..];
        return value.find(quote).map(|end| decode(&value[..end]));
    }
    None
}

/// Plain text: entities decoded, spaces collapsed, controls that could reorder a
/// line removed (as `scripts/csl-bundle.mjs` refuses them in titles). Markup
/// inside the element means it is not plain text, so it is left out.
fn text(content: &str) -> Option<String> {
    if content.contains('<') {
        return None;
    }
    let decoded = decode(content);
    let words: Vec<&str> = decoded.split_whitespace().collect();
    let text: String = words
        .join(" ")
        .chars()
        .filter(|ch| !ch.is_control() && !matches!(*ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{206f}'))
        .collect();
    (!text.is_empty()).then_some(text)
}

/// Only a web address is passed on, so a credit can never carry a script or file link.
fn web_address(uri: &str) -> Option<String> {
    let uri = text(uri)?;
    let lower = uri.to_ascii_lowercase();
    ((lower.starts_with("https://") || lower.starts_with("http://")) && !uri.contains(char::is_whitespace))
        .then_some(uri)
}

fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let entity = rest[1..].find(';').map(|end| &rest[1..=end]);
        let value = entity.and_then(|entity| match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|decimal| decimal.parse().ok()))
                .and_then(char::from_u32),
        });
        match (entity, value) {
            (Some(entity), Some(value)) => {
                out.push(value);
                rest = &rest[entity.len() + 2..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const STYLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0">
  <info>
    <title>Invented Journal of Examples</title>
    <link href="http://www.zotero.org/styles/invented" rel="self"/>
    <link href="https://example.org/guide?a=1&amp;b=2" rel="documentation"/>
    <link href="javascript:alert(1)" rel="documentation"/>
    <!-- <author><name>Commented Out</name></author> -->
    <author>
      <name>Avery  Taylor &amp; Co</name>
      <email>avery@example.org</email>
      <uri>https://example.org/avery</uri>
    </author>
    <contributor><name>Morgan Dal&#xe9;&#x202e;</name><uri>file:///etc/passwd</uri></contributor>
    <contributor><name><b>Marked</b></name></contributor>
    <summary>An invented style.</summary>
    <rights license="http://creativecommons.org/licenses/by-sa/3.0/">This work is licensed under a Creative Commons Attribution-ShareAlike 3.0 License</rights>
  </info>
</style>"#;

    #[test]
    fn credits_keep_names_and_web_pages_and_drop_emails_markup_and_other_links() {
        let info = Info::read(STYLE);
        assert_eq!(
            info.authors,
            [Person {
                name: "Avery Taylor & Co".into(),
                uri: Some("https://example.org/avery".into())
            }]
        );
        assert_eq!(
            info.contributors,
            [Person {
                name: "Morgan Dalé".into(),
                uri: None
            }]
        );
        assert_eq!(info.summary.as_deref(), Some("An invented style."));
        assert_eq!(info.documentation, ["https://example.org/guide?a=1&b=2"]);
        assert_eq!(
            info.license.as_deref(),
            Some("http://creativecommons.org/licenses/by-sa/3.0/")
        );
        assert!(info.rights.unwrap().starts_with("This work is licensed"));
    }

    #[test]
    fn a_style_without_info_credits_no_one() {
        assert_eq!(Info::read("<style><citation/></style>"), Info::default());
        assert_eq!(Info::read("<style><info><author><name>Unclosed"), Info::default());
    }
}
