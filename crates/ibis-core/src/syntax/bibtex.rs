//! BibTeX built only from share-safe CSL metadata. Every field is literal text;
//! only this writer supplies entry syntax and TeX commands.
use std::io::{self, Write};

use serde_json::{Map, Value};

pub(super) fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || "-_.:+/".contains(c))
}

/// `key` was checked by the catalogue resolver; `fields` passed `share_fields`.
/// Stream to the caller's bounded destination, including while escaping.
pub(super) fn write(out: &mut impl Write, key: &str, fields: &Map<String, Value>) -> io::Result<()> {
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
    writeln!(out, "@{kind}{{{key},")?;
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
        ("DOI", "doi"),
        ("URL", "url"),
        ("ISBN", "isbn"),
        ("ISSN", "issn"),
        ("PMID", "pmid"),
        ("PMCID", "pmcid"),
        ("abstract", "abstract"),
        ("keyword", "keywords"),
        ("language", "langid"),
        ("genre", "type"),
        ("version", "version"),
        ("event-title", "eventtitle"),
    ] {
        if let Some(value) = fields.get(csl) {
            match value {
                Value::String(text)
                    if matches!(
                        csl,
                        "title" | "title-short" | "container-title" | "container-title-short"
                    ) =>
                {
                    write!(out, "  {bib} = {{")?;
                    rich(out, text)?;
                    out.write_all(b"},\n")?;
                }
                Value::String(text) => field(out, bib, text)?,
                Value::Number(number) => field(out, bib, &number.to_string())?,
                _ => {}
            }
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
        if let Some(names) = fields.get(csl).and_then(Value::as_array) {
            write!(out, "  {bib} = {{")?;
            for (index, name) in names.iter().enumerate() {
                if index > 0 {
                    out.write_all(b" and ")?;
                }
                let part = |key| name.get(key).and_then(Value::as_str).unwrap_or_default();
                if !part("literal").is_empty() {
                    // Organizations stay one name even when they contain `and`.
                    out.write_all(b"{")?;
                    escape(out, part("literal"))?;
                    out.write_all(b"}")?;
                } else {
                    let prefix = if !part("non-dropping-particle").is_empty() {
                        Some((part("non-dropping-particle"), true))
                    } else if !part("dropping-particle").is_empty() {
                        Some((part("dropping-particle"), false))
                    } else {
                        None
                    };
                    if let Some((prefix, use_prefix)) = prefix {
                        // Extended names retain whether the prefix belongs
                        // to the family name, without trusting delimiters.
                        out.write_all(b"family={")?;
                        escape(out, part("family"))?;
                        out.write_all(b"}, given={")?;
                        escape(out, part("given"))?;
                        out.write_all(b"}, prefix={")?;
                        escape(out, prefix)?;
                        write!(out, "}}, useprefix={use_prefix}")?;
                        if !part("suffix").is_empty() {
                            out.write_all(b", suffix={")?;
                            escape(out, part("suffix"))?;
                            out.write_all(b"}")?;
                        }
                    } else {
                        // Protected components cannot introduce another
                        // author or comma-delimited name part.
                        out.write_all(b"{")?;
                        escape(out, part("family"))?;
                        out.write_all(b"}, ")?;
                        if !part("suffix").is_empty() {
                            out.write_all(b"{")?;
                            escape(out, part("suffix"))?;
                            out.write_all(b"}, ")?;
                        }
                        out.write_all(b"{")?;
                        escape(out, part("given"))?;
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
            if let Some(parts) = date
                .get("date-parts")
                .and_then(Value::as_array)
                .filter(|parts| !parts.is_empty())
            {
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
fn rich(out: &mut impl Write, text: &str) -> io::Result<()> {
    const TAGS: [(&str, &str, &str); 7] = [
        ("<i>", "</i>", "\\textit{"),
        ("<b>", "</b>", "\\textbf{"),
        ("<sup>", "</sup>", "\\textsuperscript{"),
        ("<sub>", "</sub>", "\\textsubscript{"),
        ("<sc>", "</sc>", "\\textsc{"),
        ("<span class=\"nocase\">", "</span>", "{"),
        ("<span style=\"font-variant:small-caps;\">", "</span>", "\\textsc{"),
    ];
    let mut rest = text;
    let mut closes: Vec<&str> = Vec::new();
    while !rest.is_empty() {
        if let Some(close) = closes.last().copied().filter(|close| rest.starts_with(close)) {
            out.write_all(b"}")?;
            closes.pop();
            rest = &rest[close.len()..];
        } else if let Some((open, close, tex)) = TAGS
            .iter()
            .find(|(open, _, _)| closes.len() < 64 && rest.starts_with(open))
        {
            out.write_all(tex.as_bytes())?;
            closes.push(*close);
            rest = &rest[open.len()..];
        } else {
            // Decode entities once, after recognizing actual markup. Encoded
            // tags stay literal, and decoded backslashes still get escaped.
            let first = rest.chars().next().map_or(0, char::len_utf8);
            let end = rest[first..].find('<').map_or(rest.len(), |offset| offset + first);
            let plain = rest[..end]
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&amp;", "&");
            for c in plain.chars() {
                if c == '-' {
                    out.write_all(b"\\mbox{-}")?;
                } else {
                    escape(out, c.encode_utf8(&mut [0; 4]))?;
                }
            }
            rest = &rest[end..];
        }
    }
    for _ in closes {
        out.write_all(b"}")?;
    }
    Ok(())
}
