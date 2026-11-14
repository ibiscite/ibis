//! BibTeX built only from share-safe CSL metadata. Every field is literal text;
//! only this writer supplies entry syntax and TeX commands.
use std::io::{self, Write};

use crate::citable::{CitableFields, TextPart, Verbatim};
use serde_json::Value;

pub(super) fn valid_key(key: &str) -> bool {
    Verbatim::key(key).is_some()
}

/// `key` was checked by the catalogue resolver.
/// Stream to the caller's bounded destination, including while escaping.
pub(super) fn write(out: &mut impl Write, key: &str, fields: &CitableFields) -> io::Result<()> {
    let key = Verbatim::key(key).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid BibTeX key"))?;
    let identifiers = fields.identifiers();
    let names = fields;
    let rich_fields = fields.rich_text();
    let fields = fields.plain();
    let kind = match fields.get("type").and_then(Value::as_str).unwrap_or_default() {
        "article-journal" | "article-magazine" | "article-newspaper" => "article",
        "book" => "book",
        "chapter" => "incollection",
        "paper-conference" => "inproceedings",
        "thesis" => "thesis",
        "report" => "report",
        "webpage" => "online",
        "dataset" => "dataset",
        "software" => "software",
        "patent" => "patent",
        "manuscript" => "unpublished",
        _ => "misc",
    };
    write!(out, "@{kind}{{")?;
    verbatim(out, &key)?;
    out.write_all(b",\n")?;
    for (csl, bib) in [
        ("title", "title"),
        ("title-short", "shorttitle"),
        (
            "container-title",
            if matches!(kind, "incollection" | "inproceedings") {
                "booktitle"
            } else {
                "journaltitle"
            },
        ),
        ("container-title-short", "shortjournal"),
        ("publisher", "publisher"),
        ("publisher-place", "location"),
        ("collection-title", "series"),
        ("volume", "volume"),
        ("issue", if kind == "article" { "number" } else { "issue" }),
        ("number", "number"),
        ("page", "pages"),
        ("edition", "edition"),
        ("abstract", "abstract"),
        ("keyword", "keywords"),
        ("language", "langid"),
        ("genre", "type"),
        ("version", "version"),
        ("event-title", "eventtitle"),
    ] {
        // An article's issue already owns BibTeX's number field.
        if csl == "number" && kind == "article" && fields.contains_key("issue") {
            continue;
        }
        if let Some(value) = fields.get(csl) {
            match value {
                Value::String(text)
                    if matches!(
                        csl,
                        "title" | "title-short" | "container-title" | "container-title-short"
                    ) =>
                {
                    write!(out, "  {bib} = {{")?;
                    if let Some(parts) = rich_fields.get(csl) {
                        rich(out, parts)?;
                    } else {
                        escape(out, text)?;
                    }
                    out.write_all(b"},\n")?;
                }
                Value::String(text) => field(out, bib, text)?,
                Value::Number(number) => field(out, bib, &number.to_string())?,
                _ => {}
            }
        }
    }
    for (csl, bib) in [
        ("DOI", "doi"),
        ("URL", "url"),
        ("ISBN", "isbn"),
        ("ISSN", "issn"),
        ("PMID", "pmid"),
        ("PMCID", "pmcid"),
    ] {
        if let Some(value) = identifiers.get(csl) {
            write!(out, "  {bib} = {{")?;
            verbatim(out, value)?;
            out.write_all(b"},\n")?;
        }
    }
    if let Some(subtype) = fields.get("type").and_then(Value::as_str).and_then(|kind| match kind {
        "article-newspaper" => Some("newspaper"),
        "article-magazine" => Some("magazine"),
        _ => None,
    }) {
        field(out, "entrysubtype", subtype)?;
    }
    for (csl, bib) in [
        ("author", "author"),
        ("editor", "editor"),
        ("translator", "translator"),
        ("container-author", "bookauthor"),
    ] {
        if !names.names(csl).is_empty() {
            write!(out, "  {bib} = {{")?;
            for (index, name) in names.names(csl).iter().enumerate() {
                if index > 0 {
                    out.write_all(b" and ")?;
                }
                let suffix = name.suffix.as_deref().unwrap_or_default();
                let (family, given, prefix) = name.bibtex_parts();
                if let Some(literal) = name.literal.as_deref().filter(|s| !s.is_empty()) {
                    // Organizations stay one name even when they contain `and`.
                    out.write_all(b"{")?;
                    escape(out, literal)?;
                    out.write_all(b"}")?;
                } else {
                    if let Some((prefix, use_prefix)) = prefix {
                        // Extended names retain whether the prefix belongs
                        // to the family name, without trusting delimiters.
                        out.write_all(b"family={")?;
                        escape(out, &family)?;
                        out.write_all(b"}, given={")?;
                        escape(out, given)?;
                        out.write_all(b"}, prefix={")?;
                        escape(out, prefix)?;
                        write!(out, "}}, useprefix={use_prefix}")?;
                        if !suffix.is_empty() {
                            out.write_all(b", suffix={")?;
                            escape(out, suffix)?;
                            out.write_all(b"}")?;
                        }
                    } else {
                        // Protected components cannot introduce another
                        // author or comma-delimited name part.
                        out.write_all(b"{")?;
                        escape(out, &family)?;
                        out.write_all(b"}, ")?;
                        if !suffix.is_empty() {
                            out.write_all(b"{")?;
                            escape(out, suffix)?;
                            out.write_all(b"}, ")?;
                        }
                        out.write_all(b"{")?;
                        escape(out, given)?;
                        out.write_all(b"}")?;
                    }
                }
            }
            out.write_all(b"},\n")?;
        }
    }
    for (csl, bib) in [
        ("issued", "date"),
        ("accessed", "urldate"),
        ("original-date", "origdate"),
        ("event-date", "eventdate"),
    ] {
        if let Some(date) = fields.get(csl) {
            if let Some(parts) = date.get("date-parts").and_then(Value::as_array).filter(|dates| {
                // Validate before emitting the field: a malformed part must
                // not turn into an empty date or a different partial date.
                !dates.is_empty()
                    && dates.len() <= 2
                    && dates.iter().all(|date| {
                        date.as_array()
                            .is_some_and(|parts| parts.len() <= 3 && parts.iter().all(|part| part.as_i64().is_some()))
                    })
                    && dates
                        .iter()
                        .any(|date| date.as_array().is_some_and(|parts| !parts.is_empty()))
            }) {
                write!(out, "  {bib} = {{")?;
                for (index, date) in parts.iter().take(2).enumerate() {
                    if index > 0 {
                        out.write_all(b"/")?;
                    }
                    if let Some(parts) = date.as_array() {
                        for (index, part) in parts.iter().take(3).enumerate() {
                            if let Some(number) = part.as_i64() {
                                if index == 0 {
                                    write!(out, "{number:04}")?;
                                } else {
                                    write!(out, "-{number:02}")?;
                                }
                            }
                        }
                    }
                }
                if date.get("circa").and_then(Value::as_bool) == Some(true) {
                    out.write_all(b"~")?;
                }
                out.write_all(b"},\n")?;
            } else if let Some(text) = date.get("literal").or_else(|| date.get("raw")).and_then(Value::as_str) {
                field(out, if csl == "issued" { "year" } else { bib }, text)?;
            }
        }
    }
    out.write_all(b"}")
}

fn field(out: &mut impl Write, name: &str, text: &str) -> io::Result<()> {
    write!(out, "  {name} = {{")?;
    escape(out, text)?;
    out.write_all(b"},\n")
}

fn verbatim(out: &mut impl Write, value: &Verbatim) -> io::Result<()> {
    out.write_all(value.as_str().as_bytes())
}

fn escape(out: &mut impl Write, text: &str) -> io::Result<()> {
    for c in text.chars() {
        match c {
            '\\' => out.write_all(b"\\textbackslash{}")?,
            '{' => out.write_all(b"\\textbraceleft{}")?,
            '}' => out.write_all(b"\\textbraceright{}")?,
            '~' => out.write_all(b"\\textasciitilde{}")?,
            '^' => out.write_all(b"\\textasciicircum{}")?,
            '#' | '$' | '%' | '&' | '_' => write!(out, "\\{c}")?,
            '-' => out.write_all(b"{-}")?,
            '\'' => out.write_all(b"\\textquotesingle{}")?,
            '`' => out.write_all(b"\\textasciigrave{}")?,
            // Control characters have no bibliographic meaning and must not
            // create a new physical entry or TeX input line.
            c if c.is_control() => out.write_all(b" ")?,
            c => out.write_all(c.encode_utf8(&mut [0; 4]).as_bytes())?,
        }
    }
    Ok(())
}

/// Recognize only CSL's trusted formatting. Everything else, including TeX
/// commands and unknown tags, goes through literal escaping. A stack supplies
/// balanced writer-owned groups even for malformed source markup.
fn rich(out: &mut impl Write, parts: &[TextPart]) -> io::Result<()> {
    const TAGS: [(&str, &str, &str); 7] = [
        ("<i>", "</i>", "\\textit{"),
        ("<b>", "</b>", "\\textbf{"),
        ("<sup>", "</sup>", "\\textsuperscript{"),
        ("<sub>", "</sub>", "\\textsubscript{"),
        ("<sc>", "</sc>", "\\textsc{"),
        ("<span class=\"nocase\">", "</span>", "{"),
        ("<span style=\"font-variant:small-caps;\">", "</span>", "\\textsc{"),
    ];
    let mut closes: Vec<&str> = Vec::new();
    for part in parts {
        let TextPart::Markup(tag) = part else {
            if let TextPart::Text(text) = part {
                for c in text.chars() {
                    if c == '-' {
                        out.write_all(b"\\mbox{-}")?;
                    } else {
                        escape(out, c.encode_utf8(&mut [0; 4]))?;
                    }
                }
            }
            continue;
        };
        if closes.last().is_some_and(|close| tag == close) {
            out.write_all(b"}")?;
            closes.pop();
        } else if let Some((_, close, tex)) = TAGS.iter().find(|(open, _, _)| closes.len() < 64 && tag == open) {
            out.write_all(tex.as_bytes())?;
            closes.push(*close);
        } else {
            escape(out, tag)?;
        }
    }
    for _ in closes {
        out.write_all(b"}")?;
    }
    Ok(())
}
