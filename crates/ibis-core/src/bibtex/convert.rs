//! Converts a parsed BibLaTeX entry into a [`Reference`], preserving meaning:
//! protected capitalization, italics, truncated author lists, organizations,
//! date ranges and uncertainty, and proper language tags.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use biblatex::{Chunk, ChunksExt, DateValue, EditorType, Entry, PermissiveType, RetrievalError};

use super::resolve::{BACKSLASH, ESCAPE, LEFT_BRACE, RIGHT_BRACE};
use crate::model::{Issued, Name, OTHERS, PartialDate, RecordId, Reference};

/// Convert one entry. Problems that do not prevent loading are pushed to `problems`.
pub(crate) fn convert(
    source: &str,
    kind: &str,
    raw: &str,
    entry: &Entry,
    own_fields: &HashMap<String, String>,
    problems: &mut Vec<String>,
) -> Reference {
    // TeX commands that are not understood are kept as written and reported.
    let unknown = RefCell::new(BTreeSet::new());
    // A field containing a TeX command that is not understood is kept exactly
    // as written (from the source text), so nothing is silently changed.
    // Raw values still carry the literal-symbol encoding. It is decoded exactly
    // once: by `clean` for plain fields, by `render` for rich ones.
    let raw_text = |field: &str| own_fields.get(field).map(|v| clean(v));
    let raw_encoded = |field: &str| {
        own_fields
            .get(field)
            .map(|v| v.split_whitespace().collect::<Vec<_>>().join(" "))
    };
    let text = |field: &str| {
        entry
            .get(field)
            .map(|c| {
                let mut found = BTreeSet::new();
                let plain = clean(&strip_tex(&c.format_verbatim(), &mut found));
                if found.is_empty() {
                    plain
                } else {
                    unknown.borrow_mut().extend(found);
                    raw_text(field).unwrap_or(plain)
                }
            })
            .filter(|s| !s.is_empty())
    };
    let field_pieces = |field: &str| -> Option<Vec<Piece>> {
        let mut found = BTreeSet::new();
        let pieces = pieces_from_chunks(entry.get(field)?.iter().map(|c| &c.v), &mut found);
        if found.is_empty() {
            return Some(pieces);
        }
        unknown.borrow_mut().extend(found);
        Some(vec![Piece::Text {
            text: raw_encoded(field).unwrap_or_default(),
            protected: true,
        }])
    };
    let rich = |main: &str, sub: &[(&str, &str)]| -> Option<Vec<Piece>> {
        let mut pieces = field_pieces(main)?;
        for (field, separator) in sub {
            if let Some(more) = field_pieces(field) {
                pieces.push(Piece::Text {
                    text: (*separator).to_string(),
                    protected: false,
                });
                pieces.extend(more);
            }
        }
        Some(pieces)
    };

    let language = language_tag(entry, problems);
    let english = language.as_deref().is_none_or(|l| l == "en" || l.starts_with("en-"));
    let title = rich("title", &[("subtitle", ": "), ("titleaddon", ". ")])
        .map(|pieces| render(&if english { sentence_case(pieces) } else { pieces }));
    let short_title = rich("shorttitle", &[])
        .map(|pieces| render(&if english { sentence_case(pieces) } else { pieces }))
        .filter(|s| !s.is_empty());
    // `booktitle` names the containing book only for parts of a book or of
    // proceedings; on a whole book or proceedings volume it repeats the title.
    let item_type = match (
        kind,
        entry
            .get("entrysubtype")
            .map(|c| c.format_verbatim().to_ascii_lowercase()),
    ) {
        ("article", Some(sub)) if sub == "newspaper" => "article-newspaper",
        ("article", Some(sub)) if sub == "magazine" => "article-magazine",
        _ => csl_type(kind),
    };
    let containers: &[(&str, &str, &str)] = if matches!(item_type, "chapter" | "paper-conference") {
        &[
            ("booktitle", "booksubtitle", "booktitleaddon"),
            ("journaltitle", "journalsubtitle", "journaltitleaddon"),
            ("journal", "journalsubtitle", "journaltitleaddon"),
        ]
    } else {
        &[
            ("journaltitle", "journalsubtitle", "journaltitleaddon"),
            ("journal", "journalsubtitle", "journaltitleaddon"),
        ]
    };
    let mut container_title = containers
        .iter()
        .find_map(|(main, sub, addon)| rich(main, &[(sub, ": "), (addon, ". ")]))
        .map(|pieces| render(&pieces))
        .filter(|s| !s.is_empty());

    // A volume of a multi-volume work: the whole work's title (`maintitle`)
    // is the CSL title of a book, or the container of a part, and the
    // volume's own title becomes `volume-title`.
    let main_title = rich("maintitle", &[("mainsubtitle", ": "), ("maintitleaddon", ". ")]);
    let mut title = title;
    let mut volume_title = None;
    if let Some(main) = main_title {
        if matches!(item_type, "chapter" | "paper-conference") {
            volume_title = container_title.take();
            container_title = Some(render(&main)).filter(|s| !s.is_empty());
        } else {
            volume_title = title.take();
            title = Some(render(&if english { sentence_case(main) } else { main }));
        }
    }

    let authors = match entry.author() {
        Ok(people) => names(people, own_fields.get("author").map(String::as_str)),
        Err(RetrievalError::Missing(_)) => Vec::new(),
        Err(err) => {
            problems.push(format!("authors could not be read: {err}"));
            Vec::new()
        }
    };
    // Editors by role. BibLaTeX roles without a CSL equivalent are exported
    // as general contributors, and the loss of detail is reported.
    let mut other_names: BTreeMap<String, Vec<Name>> = BTreeMap::new();
    let mut editors = Vec::new();
    match entry.editors() {
        Ok(groups) => {
            for (people, role) in groups {
                let converted = names(people, None);
                let csl_role = match &role {
                    EditorType::Editor => {
                        editors.extend(converted);
                        continue;
                    }
                    EditorType::Compiler => "compiler",
                    EditorType::Director => "director",
                    EditorType::Organizer => "organizer",
                    EditorType::Collaborator => "contributor",
                    other => {
                        problems.push(format!(
                            "editor role `{}` has no CSL equivalent; those names are exported as contributors",
                            editor_role_name(other)
                        ));
                        "contributor"
                    }
                };
                other_names.entry(csl_role.to_string()).or_default().extend(converted);
            }
        }
        Err(err) => problems.push(format!("editors could not be read: {err}")),
    }
    let roles: [(&str, &str); 7] = [
        ("translator", "translator"),
        ("bookauthor", "container-author"),
        ("annotator", "contributor"),
        ("commentator", "contributor"),
        ("introduction", "contributor"),
        ("foreword", "contributor"),
        ("afterword", "contributor"),
    ];
    for (field, csl_role) in roles {
        match entry.get_as::<Vec<biblatex::Person>>(field) {
            Ok(people) => {
                if csl_role == "contributor" {
                    problems.push(format!(
                        "role `{field}` has no CSL equivalent; those names are exported as contributors"
                    ));
                }
                other_names
                    .entry(csl_role.to_string())
                    .or_default()
                    .extend(names(people, own_fields.get(field).map(String::as_str)));
            }
            Err(RetrievalError::Missing(_)) => {}
            Err(err) => problems.push(format!("`{field}` could not be read: {err}")),
        }
    }

    let mut variables = BTreeMap::new();
    let mut set = |csl: &str, fields: &[&str]| {
        if let Some(value) = fields.iter().find_map(|f| text(f)) {
            variables.insert(csl.to_string(), value);
        }
    };
    set("volume", &["volume"]);
    if kind == "article" {
        set("issue", &["number", "issue"]);
    } else if entry.get("series").is_some() && !matches!(kind, "report" | "techreport") {
        // In a book series, `number` is the volume's number within the series.
        set("collection-number", &["number"]);
        set("issue", &["issue"]);
    } else {
        set("number", &["number"]);
        set("issue", &["issue"]);
    }
    set("page", &["pages"]);
    set("publisher", &["publisher", "institution", "school", "organization"]);
    set("publisher-place", &["location", "address"]);
    set("edition", &["edition"]);
    set("ISBN", &["isbn"]);
    set("ISSN", &["issn"]);
    set("collection-title", &["series"]);
    set("genre", &["type"]);
    set("note", &["note"]);
    set("event-title", &["eventtitle"]);
    if let Some(short_title) = short_title {
        variables.insert("title-short".to_string(), short_title);
    }
    if let Some(volume_title) = volume_title {
        variables.insert("volume-title".to_string(), volume_title);
    }
    let genre = variables.get("genre").map(String::as_str).or(match kind {
        "phdthesis" => Some("phdthesis"),
        "mastersthesis" => Some("mathesis"),
        "techreport" => Some("techreport"),
        _ => None,
    });
    if let Some(readable) = genre.and_then(readable_genre) {
        variables.insert("genre".to_string(), readable.to_string());
    }

    let issued = issued(entry, problems);
    let dates = other_dates(entry, problems);
    let doi = text("doi");
    let abstract_text = text("abstract");
    let keywords: Vec<String> = text("keywords")
        .map(|k| {
            k.split([',', ';'])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let unknown = unknown.into_inner();
    if !unknown.is_empty() {
        let list = unknown
            .iter()
            .map(|c| format!("`\\{c}`"))
            .collect::<Vec<_>>()
            .join(", ");
        problems.push(format!("TeX command {list} is not understood; it is kept as written"));
    }

    Reference {
        id: RecordId {
            source: source.to_string(),
            native_key: entry.key.clone(),
        },
        citekey: Some(entry.key.clone()),
        item_type: item_type.to_string(),
        title,
        authors,
        editors,
        other_names,
        issued,
        dates,
        container_title,
        doi,
        url: entry
            .get("url")
            .map(|c| clean(&c.format_verbatim()))
            .filter(|s| !s.is_empty()),
        abstract_text,
        keywords,
        language,
        variables,
        raw: Some(raw.to_string()),
    }
}

// ---------- Names ----------

fn editor_role_name(role: &EditorType) -> String {
    match role {
        EditorType::Unknown(name) => name.clone(),
        other => format!("{other:?}").to_lowercase(),
    }
}

/// Convert parsed people, using the raw field to recognize names written in
/// braces (organizations) and the "and others" marker.
fn names(people: Vec<biblatex::Person>, raw: Option<&str>) -> Vec<Name> {
    let braced = raw.map(braced_names).unwrap_or_default();
    people
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            let given = clean(&p.given_name);
            let family = clean(&p.name);
            if family == OTHERS && given.is_empty() && p.prefix.is_empty() {
                return Name::literal(OTHERS);
            }
            let is_braced = braced.get(i).copied().unwrap_or(false);
            if is_braced || (raw.is_none() && given.is_empty() && family.contains(' ')) {
                return Name::literal(&family);
            }
            let some = |s: String| (!s.is_empty()).then_some(s);
            Name {
                family: some(family),
                given: some(given),
                dropping_particle: some(clean(&p.prefix)),
                suffix: some(clean(&p.suffix)),
                ..Name::default()
            }
        })
        .collect()
}

/// For each name in an `a and b and c` list, whether it is wholly in braces.
fn braced_names(raw: &str) -> Vec<bool> {
    let mut result = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b' ' | b'\t' | b'\n' if depth == 0 => {
                let rest = &raw[i..];
                let trimmed = rest.trim_start();
                let t = trimmed.as_bytes();
                // Byte comparison: "and" is ASCII, so this never splits a character.
                if t.len() >= 4 && t[..3].eq_ignore_ascii_case(b"and") && t[3].is_ascii_whitespace() {
                    result.push(wholly_braced(&raw[start..i]));
                    start = i + (rest.len() - trimmed.len()) + 4;
                    i = start;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    result.push(wholly_braced(&raw[start..]));
    result
}

fn wholly_braced(name: &str) -> bool {
    let name = name.trim();
    if !(name.starts_with('{') && name.ends_with('}')) {
        return false;
    }
    let mut depth = 0usize;
    for (i, c) in name.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 && i != name.len() - 1 {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

// ---------- Dates ----------

fn issued(entry: &Entry, problems: &mut Vec<String>) -> Option<Issued> {
    date_value(entry.date(), entry, "date", problems)
}

/// Further CSL date variables: access date, original date, event date.
fn other_dates(entry: &Entry, problems: &mut Vec<String>) -> BTreeMap<String, Issued> {
    let sources = [
        ("accessed", entry.url_date(), "urldate"),
        ("original-date", entry.orig_date(), "origdate"),
        ("event-date", entry.event_date(), "eventdate"),
    ];
    sources
        .into_iter()
        .filter_map(|(csl, value, field)| date_value(value, entry, field, problems).map(|d| (csl.to_string(), d)))
        .collect()
}

fn date_value(
    value: Result<PermissiveType<biblatex::Date>, RetrievalError>,
    entry: &Entry,
    field: &str,
    problems: &mut Vec<String>,
) -> Option<Issued> {
    match value {
        Ok(PermissiveType::Typed(date)) => {
            let to_partial = |d: biblatex::Datetime| PartialDate {
                year: d.year,
                // The parser counts months and days from zero; CSL counts from one.
                month: d.month.map(|m| m + 1),
                day: d.day.map(|day| day + 1),
            };
            let raw = entry
                .get(field)
                .map(|c| c.format_verbatim())
                .unwrap_or_else(|| "year".to_string());
            let (start, end) = match date.value {
                DateValue::At(d) => (to_partial(d), None),
                DateValue::Between(a, b) => (to_partial(a), Some(to_partial(b))),
                DateValue::Before(d) => {
                    problems.push(format!(
                        "date `{raw}` has no start; CSL cannot express that, so only its end is used"
                    ));
                    (to_partial(d), None)
                }
                DateValue::After(d) => {
                    problems.push(format!(
                        "date `{raw}` has no end; CSL cannot express that, so only its start is used"
                    ));
                    (to_partial(d), None)
                }
            };
            if date.uncertain {
                problems.push(format!(
                    "date `{raw}` is marked uncertain; CSL cannot express that, so it is used without the mark"
                ));
            }
            Some(Issued {
                start,
                end,
                circa: date.approximate,
            })
        }
        Ok(PermissiveType::Chunks(chunks)) => {
            let raw = chunks.format_verbatim();
            match lenient_date(&raw) {
                Some((date, complete)) => {
                    if !complete {
                        problems.push(format!(
                            "date `{raw}` is not a valid date; only {} is used",
                            describe(date)
                        ));
                    }
                    Some(Issued {
                        start: date,
                        end: None,
                        circa: false,
                    })
                }
                None => {
                    problems.push(format!("date `{raw}` could not be read; no date is used"));
                    None
                }
            }
        }
        Err(RetrievalError::Missing(_)) => None,
        Err(err) => {
            problems.push(format!("`{field}` could not be read: {err}"));
            None
        }
    }
}

/// Read as much of `YYYY[-MM[-DD]]` as is valid. Returns the date and
/// whether the whole text was a valid date.
fn lenient_date(text: &str) -> Option<(PartialDate, bool)> {
    let text = text.trim();
    let year_digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    if year_digits.len() != 4 {
        return None;
    }
    let year: i32 = year_digits.parse().ok()?;
    let mut date = PartialDate::year(year);
    let mut parts = text[4..].split('-').skip(1);
    let rest_is_date = text[4..].chars().all(|c| c.is_ascii_digit() || c == '-');
    let month = parts
        .next()
        .and_then(|m| m.parse::<u8>().ok())
        .filter(|m| (1..=12).contains(m));
    let day_text = parts.next();
    if let Some(m) = month {
        date.month = Some(m);
        if let Some(d) = day_text.and_then(|d| d.parse::<u8>().ok())
            && d >= 1
            && d <= days_in_month(year, m)
        {
            date.day = Some(d);
        }
    }
    let expected = 1 + usize::from(date.month.is_some()) + usize::from(date.day.is_some());
    let given = text.split('-').filter(|s| !s.is_empty()).count();
    Some((date, rest_is_date && expected == given))
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn describe(date: PartialDate) -> String {
    match (date.month, date.day) {
        (Some(m), Some(d)) => format!("{:04}-{m:02}-{d:02}", date.year),
        (Some(m), None) => format!("{:04}-{m:02}", date.year),
        _ => format!("the year {}", date.year),
    }
}

// ---------- Language ----------

/// BCP 47 tag from `langid` or `language`. Babel/polyglossia names are
/// mapped; tags that already look like BCP 47 are kept; anything else is
/// reported and left out rather than passed on as a wrong tag.
fn language_tag(entry: &Entry, problems: &mut Vec<String>) -> Option<String> {
    let raw = ["langid", "language"]
        .iter()
        .find_map(|f| entry.get(f).map(|c| clean(&c.format_verbatim())))
        .filter(|s| !s.is_empty())?;
    let lower = raw.to_ascii_lowercase();
    if let Some(tag) = babel_to_bcp47(&lower) {
        return Some(tag.to_string());
    }
    let looks_like_tag = {
        let mut parts = raw.split('-');
        let primary = parts.next().unwrap_or_default();
        (2..=3).contains(&primary.len())
            && primary.chars().all(|c| c.is_ascii_alphabetic())
            && parts.all(|p| (2..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
    };
    if looks_like_tag {
        return Some(raw);
    }
    problems.push(format!("language `{raw}` is not recognized; it is left out"));
    None
}

fn babel_to_bcp47(name: &str) -> Option<&'static str> {
    Some(match name {
        "american" | "usenglish" | "english" => "en-US",
        "british" | "ukenglish" => "en-GB",
        "australian" => "en-AU",
        "canadian" => "en-CA",
        "newzealand" => "en-NZ",
        "german" | "ngerman" => "de-DE",
        "austrian" | "naustrian" => "de-AT",
        "swissgerman" | "nswissgerman" => "de-CH",
        "french" | "francais" => "fr-FR",
        "acadian" | "canadien" => "fr-CA",
        "spanish" => "es-ES",
        "mexican" => "es-MX",
        "italian" => "it-IT",
        "portuguese" | "portuges" => "pt-PT",
        "brazil" | "brazilian" => "pt-BR",
        "dutch" => "nl-NL",
        "danish" => "da-DK",
        "swedish" => "sv-SE",
        "norwegian" | "norsk" | "bokmal" => "nb-NO",
        "nynorsk" => "nn-NO",
        "finnish" => "fi-FI",
        "icelandic" => "is-IS",
        "polish" => "pl-PL",
        "czech" => "cs-CZ",
        "slovak" => "sk-SK",
        "slovene" | "slovenian" => "sl-SI",
        "croatian" => "hr-HR",
        "serbian" => "sr-RS",
        "bulgarian" => "bg-BG",
        "russian" => "ru-RU",
        "ukrainian" => "uk-UA",
        "greek" => "el-GR",
        "turkish" => "tr-TR",
        "hungarian" | "magyar" => "hu-HU",
        "romanian" => "ro-RO",
        "catalan" => "ca-AD",
        "basque" => "eu",
        "galician" => "gl",
        "estonian" => "et-EE",
        "latvian" => "lv-LV",
        "lithuanian" => "lt-LT",
        "hebrew" => "he-IL",
        "arabic" => "ar",
        "persian" | "farsi" => "fa-IR",
        "hindi" => "hi-IN",
        "bengali" => "bn-IN",
        "chinese" => "zh-CN",
        "japanese" => "ja-JP",
        "korean" => "ko-KR",
        "vietnamese" => "vi-VN",
        "thai" => "th-TH",
        "indonesian" | "bahasa" => "id-ID",
        "malay" => "ms-MY",
        "latin" => "la",
        "afrikaans" => "af-ZA",
        "welsh" => "cy-GB",
        "irish" => "ga-IE",
        _ => return None,
    })
}

// ---------- Titles: TeX to CSL rich text, sentence case ----------

#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text { text: String, protected: bool },
    Tag(&'static str),
}

/// Turn parser chunks into text pieces and markup tags. Braced (verbatim)
/// chunks are protected from case changes and wrapped in a `nocase` span.
fn pieces_from_chunks<'a>(chunks: impl Iterator<Item = &'a Chunk>, unknown: &mut BTreeSet<String>) -> Vec<Piece> {
    let mut pieces = Vec::new();
    for chunk in chunks {
        match chunk {
            Chunk::Normal(s) => tex_pieces(s, false, true, &mut pieces, unknown),
            Chunk::Verbatim(s) => {
                pieces.push(Piece::Tag("<span class=\"nocase\">"));
                tex_pieces(s, true, true, &mut pieces, unknown);
                pieces.push(Piece::Tag("</span>"));
            }
            Chunk::Math(s) => math_pieces(s, &mut pieces, unknown),
        }
    }
    pieces
}

/// Math: symbols as in text, plus `^`/`_` as superscript and subscript.
/// Math is protected from case changes.
fn math_pieces(math: &str, out: &mut Vec<Piece>, unknown: &mut BTreeSet<String>) {
    let mut rest = math;
    let mut plain = String::new();
    while let Some(pos) = rest.find(['^', '_']) {
        plain.push_str(&rest[..pos]);
        let (open, close) = if rest[pos..].starts_with('^') {
            ("<sup>", "</sup>")
        } else {
            ("<sub>", "</sub>")
        };
        let after = &rest[pos + 1..];
        let (script, remainder) = if let Some(inner) = after.strip_prefix('{') {
            let mut depth = 1usize;
            let end = inner
                .char_indices()
                .find(|(_, c)| {
                    match c {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                    depth == 0
                })
                .map_or(inner.len(), |(i, _)| i);
            (&inner[..end], inner.get(end + 1..).unwrap_or(""))
        } else if let Some(command) = after.strip_prefix('\\') {
            let len = 1 + command
                .chars()
                .take_while(char::is_ascii_alphabetic)
                .map(char::len_utf8)
                .sum::<usize>();
            (&after[..len], &after[len..])
        } else {
            let len = after.chars().next().map_or(0, char::len_utf8);
            (&after[..len], &after[len..])
        };
        tex_pieces(&std::mem::take(&mut plain), true, true, out, unknown);
        out.push(Piece::Tag(open));
        tex_pieces(script, true, true, out, unknown);
        out.push(Piece::Tag(close));
        rest = remainder;
    }
    plain.push_str(rest);
    tex_pieces(&plain, true, true, out, unknown);
}

/// Commands whose argument is shown as-is.
const TRANSPARENT: [&str; 16] = [
    "ensuremath",
    "mathrm",
    "mathit",
    "mathbf",
    "mathsf",
    "mathtt",
    "mathcal",
    "mathnormal",
    "text",
    "textrm",
    "textup",
    "textnormal",
    "mbox",
    "hbox",
    "url",
    "nolinkurl",
];

/// Formatting commands that take an argument, and their CSL tags.
fn formatting(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "emph" | "textit" | "textsl" | "mkbibemph" | "mkbibitalic" => ("<i>", "</i>"),
        "textbf" | "mkbibbold" => ("<b>", "</b>"),
        "textsc" => ("<span style=\"font-variant:small-caps;\">", "</span>"),
        "textsuperscript" | "mkbibsuperscript" => ("<sup>", "</sup>"),
        "textsubscript" | "mkbibsubscript" => ("<sub>", "</sub>"),
        _ => return None,
    })
}

/// Old-style declarations such as `{\em text}` that apply to the rest of a group.
fn declaration(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "em" | "it" | "itshape" | "sl" | "slshape" => ("<i>", "</i>"),
        "bf" | "bfseries" => ("<b>", "</b>"),
        "sc" | "scshape" => ("<span style=\"font-variant:small-caps;\">", "</span>"),
        _ => return None,
    })
}

/// Unicode for common text and math symbol commands.
fn tex_symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ϵ",
        "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" => "π",
        "varpi" => "ϖ",
        "rho" => "ρ",
        "varrho" => "ϱ",
        "sigma" => "σ",
        "varsigma" => "ς",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "ϕ",
        "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Upsilon" => "Υ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "times" => "×",
        "pm" => "±",
        "mp" => "∓",
        "cdot" => "·",
        "div" => "÷",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "ne" | "neq" => "≠",
        "approx" => "≈",
        "sim" => "∼",
        "simeq" => "≃",
        "equiv" => "≡",
        "propto" => "∝",
        "infty" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "sum" => "∑",
        "prod" => "∏",
        "int" => "∫",
        "sqrt" => "√",
        "to" | "rightarrow" => "→",
        "leftarrow" => "←",
        "Rightarrow" => "⇒",
        "Leftarrow" => "⇐",
        "leftrightarrow" => "↔",
        "Leftrightarrow" => "⇔",
        "uparrow" => "↑",
        "downarrow" => "↓",
        "in" => "∈",
        "notin" => "∉",
        "subset" => "⊂",
        "subseteq" => "⊆",
        "cup" => "∪",
        "cap" => "∩",
        "forall" => "∀",
        "exists" => "∃",
        "emptyset" | "varnothing" => "∅",
        "circ" => "∘",
        "prime" => "′",
        "ell" => "ℓ",
        "hbar" => "ℏ",
        "degree" | "textdegree" => "°",
        "ldots" | "dots" | "textellipsis" => "…",
        "textendash" => "–",
        "textemdash" => "—",
        "textquoteleft" => "‘",
        "textquoteright" => "’",
        "textquotedblleft" => "“",
        "textquotedblright" => "”",
        "guillemotleft" => "«",
        "guillemotright" => "»",
        "S" | "textsection" => "§",
        "P" | "textparagraph" => "¶",
        "copyright" | "textcopyright" => "©",
        "textregistered" => "®",
        "texttrademark" => "™",
        "euro" | "texteuro" => "€",
        "pounds" | "textsterling" => "£",
        "yen" | "textyen" => "¥",
        "textmu" | "micro" => "µ",
        "dag" | "textdagger" => "†",
        "ddag" | "textdaggerdbl" => "‡",
        "textbullet" => "•",
        "ae" => "æ",
        "AE" => "Æ",
        "oe" => "œ",
        "OE" => "Œ",
        "o" => "ø",
        "O" => "Ø",
        "aa" => "å",
        "AA" => "Å",
        "ss" => "ß",
        "l" => "ł",
        "L" => "Ł",
        "i" => "ı",
        "LaTeX" => "LaTeX",
        "TeX" => "TeX",
        "BibTeX" => "BibTeX",
        "textbackslash" => "\\",
        "textasciitilde" => "~",
        "textasciicircum" => "^",
        "textunderscore" => "_",
        _ => return None,
    })
}

/// Translate TeX in a field: formatting commands and declarations become CSL
/// tags, symbol commands become Unicode, wrapper commands keep their
/// argument, and anything else is kept as written and recorded in `unknown`.
fn tex_pieces(
    text: &str,
    protected: bool,
    grouping_braces: bool,
    out: &mut Vec<Piece>,
    unknown: &mut BTreeSet<String>,
) {
    let mut buf = String::new();
    let mut chars = text.char_indices().peekable();
    // Open brace groups; index 0 is the whole text.
    let mut groups: Vec<Group> = vec![Group::default()];
    let flush = |buf: &mut String, out: &mut Vec<Piece>| {
        if !buf.is_empty() {
            out.push(Piece::Text {
                text: std::mem::take(buf),
                protected,
            });
        }
    };
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                let name: String = text[i + 1..].chars().take_while(char::is_ascii_alphabetic).collect();
                if name.is_empty() {
                    // Control symbols: spacing commands and escaped characters.
                    match chars.next().map(|(_, s)| s) {
                        Some(',' | ';' | ':' | ' ' | '~' | '\\') => buf.push(' '),
                        Some('!' | '-' | '/') | None => {}
                        Some(other) => buf.push(other),
                    }
                    continue;
                }
                for _ in 0..name.chars().count() {
                    chars.next();
                }
                if let Some(symbol) = tex_symbol(&name) {
                    buf.push_str(symbol);
                    // `\alpha{}` or `\alpha ` ends the command name.
                    let mut ahead = chars.clone();
                    if ahead.next().is_some_and(|(_, c)| c == '{') && ahead.next().is_some_and(|(_, c)| c == '}') {
                        chars.next();
                        chars.next();
                    } else if chars.peek().is_some_and(|(_, c)| *c == ' ') {
                        chars.next();
                    }
                    continue;
                }
                if let Some((open, close)) = declaration(&name) {
                    flush(&mut buf, out);
                    out.push(Piece::Tag(open));
                    if let Some(group) = groups.last_mut() {
                        group.closes.push(close);
                    }
                    if chars.peek().is_some_and(|(_, c)| *c == ' ') {
                        chars.next();
                    }
                    continue;
                }
                let mut skipped_space = false;
                while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
                    chars.next();
                    skipped_space = true;
                }
                let has_argument = chars.peek().is_some_and(|(_, c)| *c == '{');
                if has_argument {
                    chars.next();
                    let mut group = Group::default();
                    if let Some((open, close)) = formatting(&name) {
                        flush(&mut buf, out);
                        out.push(Piece::Tag(open));
                        group.closes.push(close);
                    } else if !TRANSPARENT.contains(&name.as_str()) {
                        // Unknown command: keep it, and its argument, as written.
                        buf.push('\\');
                        buf.push_str(&name);
                        buf.push('{');
                        group.literal = true;
                        unknown.insert(name);
                    }
                    groups.push(group);
                } else {
                    // Unknown command without an argument: keep it visible.
                    unknown.insert(name.clone());
                    buf.push('\\');
                    buf.push_str(&name);
                    if skipped_space {
                        buf.push(' ');
                    }
                }
            }
            '{' if grouping_braces => groups.push(Group::default()),
            '{' => {
                buf.push('{');
                if let Some(group) = groups.last_mut() {
                    group.literal_depth += 1;
                }
            }
            '}' => {
                let literal_close = groups.last().is_some_and(|g| g.literal_depth > 0);
                if literal_close {
                    buf.push('}');
                    if let Some(group) = groups.last_mut() {
                        group.literal_depth -= 1;
                    }
                } else if groups.len() > 1 {
                    if let Some(group) = groups.pop() {
                        if group.literal {
                            buf.push('}');
                            // A brace group right after is the command's next argument.
                            if chars.peek().is_some_and(|(_, c)| *c == '{') {
                                chars.next();
                                buf.push('{');
                                groups.push(Group {
                                    literal: true,
                                    ..Group::default()
                                });
                                continue;
                            }
                        }
                        if !group.closes.is_empty() {
                            flush(&mut buf, out);
                            out.extend(group.closes.into_iter().rev().map(Piece::Tag));
                        }
                    }
                } else if !grouping_braces {
                    buf.push('}');
                }
            }
            _ => buf.push(c),
        }
    }
    flush(&mut buf, out);
    for group in groups.into_iter().rev() {
        out.extend(group.closes.into_iter().rev().map(Piece::Tag));
    }
}

/// An open brace group while translating TeX.
#[derive(Default)]
struct Group {
    /// CSL tags to close when the group ends.
    closes: Vec<&'static str>,
    /// The group is the argument of an unknown command kept as written.
    literal: bool,
    /// Literal `{` characters opened inside this group (non-grouping mode).
    literal_depth: usize,
}

/// Convert BibTeX title case to CSL sentence case (English only): capitalized
/// words become lowercase except at the start of the title or after `:`, `?`
/// or `!`. Protected text, acronyms, mixed-case words and initials ("E.")
/// keep their capitalization.
fn sentence_case(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut at_start = true;
    pieces
        .into_iter()
        .map(|piece| match piece {
            Piece::Text { text, protected: true } => {
                if text.chars().any(|c| !c.is_whitespace()) {
                    at_start = false;
                }
                Piece::Text { text, protected: true }
            }
            Piece::Text { text, protected: false } => {
                let mut out = String::with_capacity(text.len());
                let mut word = String::new();
                for c in text.chars() {
                    if c.is_whitespace() {
                        out.push_str(&convert_word(&word, &mut at_start));
                        word.clear();
                        out.push(c);
                    } else {
                        word.push(c);
                    }
                }
                out.push_str(&convert_word(&word, &mut at_start));
                Piece::Text {
                    text: out,
                    protected: false,
                }
            }
            tag => tag,
        })
        .collect()
}

fn convert_word(word: &str, at_start: &mut bool) -> String {
    if word.is_empty() {
        return String::new();
    }
    let start = *at_start;
    *at_start = word.ends_with([':', '?', '!']);
    let mut first_part = true;
    word.split('-')
        .map(|part| {
            let keep = (start && first_part) || !is_plain_capitalized(part);
            first_part = false;
            if keep { part.to_string() } else { lowercase_first(part) }
        })
        .collect::<Vec<_>>()
        .join("-")
}

/// "Memory" or "A": one capital followed only by lowercase letters. Initials
/// such as "E." are excluded.
fn is_plain_capitalized(part: &str) -> bool {
    let mut letters = part.chars().filter(|c| c.is_alphabetic());
    let Some(first) = letters.next() else { return false };
    if !first.is_uppercase() || !part.starts_with(first) {
        return false;
    }
    let rest_lower = letters.all(char::is_lowercase);
    let is_initial = part.chars().filter(|c| c.is_alphabetic()).count() == 1 && part.ends_with('.');
    rest_lower && !is_initial
}

fn lowercase_first(part: &str) -> String {
    let mut chars = part.chars();
    chars
        .next()
        .map(|c| c.to_lowercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Render pieces as CSL rich text, escaping markup characters in text.
fn render(pieces: &[Piece]) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            Piece::Text { text, .. } => {
                for c in text.chars() {
                    match c {
                        '&' => out.push_str("&amp;"),
                        '<' => out.push_str("&lt;"),
                        '>' => out.push_str("&gt;"),
                        c => out.push(c),
                    }
                }
            }
            Piece::Tag(tag) => out.push_str(tag),
        }
    }
    let out = out.replace("<span class=\"nocase\"></span>", "");
    clean(&out)
}

// ---------- Small helpers ----------

/// Plain text of a field: TeX symbols decoded, formatting removed.
fn strip_tex(text: &str, unknown: &mut BTreeSet<String>) -> String {
    let mut pieces = Vec::new();
    tex_pieces(text, false, true, &mut pieces, unknown);
    pieces
        .into_iter()
        .filter_map(|p| match p {
            Piece::Text { text, .. } => Some(text),
            Piece::Tag(_) => None,
        })
        .collect()
}

/// Collapse internal whitespace (BibTeX values often span lines) and turn
/// literal-symbol placeholders back into literal braces and backslashes.
fn clean(s: &str) -> String {
    let joined = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::with_capacity(joined.len());
    let mut chars = joined.chars();
    while let Some(c) = chars.next() {
        match c {
            ESCAPE => out.extend(chars.next()),
            LEFT_BRACE => out.push('{'),
            RIGHT_BRACE => out.push('}'),
            BACKSLASH => out.push('\\'),
            c => out.push(c),
        }
    }
    out
}

/// English text for the BibLaTeX type keys that describe theses and reports.
fn readable_genre(key: &str) -> Option<&'static str> {
    Some(match key.to_ascii_lowercase().as_str() {
        "phdthesis" => "PhD thesis",
        "mathesis" | "mastersthesis" => "Master's thesis",
        "candthesis" => "Candidate thesis",
        "techreport" => "Technical report",
        "resreport" => "Research report",
        "software" => "Computer software",
        "datacd" => "CD-ROM",
        "audiocd" => "Audio CD",
        _ => return None,
    })
}

/// Map a BibTeX/BibLaTeX entry type to the closest CSL item type.
pub fn csl_type(kind: &str) -> &'static str {
    match kind {
        "article" => "article-journal",
        "book" | "mvbook" | "proceedings" | "mvproceedings" | "collection" | "mvcollection" | "booklet" | "manual" => {
            "book"
        }
        "inbook" | "incollection" | "bookinbook" | "suppbook" | "suppcollection" => "chapter",
        "inproceedings" | "conference" => "paper-conference",
        "thesis" | "phdthesis" | "mastersthesis" => "thesis",
        "report" | "techreport" => "report",
        "online" | "www" | "electronic" => "webpage",
        "dataset" => "dataset",
        "software" => "software",
        "patent" => "patent",
        "unpublished" => "manuscript",
        _ => "document",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title(chunks: &[Chunk], english: bool) -> String {
        let pieces = pieces_from_chunks(chunks.iter(), &mut BTreeSet::new());
        render(&if english { sentence_case(pieces) } else { pieces })
    }

    #[test]
    fn sentence_case_keeps_protected_words_acronyms_and_initials() {
        let chunks = [
            Chunk::Normal("Using ".into()),
            Chunk::Verbatim("iPhone".into()),
            Chunk::Normal(" Apps for DNA Repair in E. coli: A Self-Supervised Review".into()),
        ];
        assert_eq!(
            title(&chunks, true),
            "Using <span class=\"nocase\">iPhone</span> apps for DNA repair in E. coli: A self-supervised review"
        );
    }

    #[test]
    fn formatting_commands_become_tags_and_non_english_is_unchanged() {
        let chunks = [
            Chunk::Normal("The Role of ".into()),
            Chunk::Verbatim("\\emph{in vivo}".into()),
            Chunk::Normal(" and \\textit{In Vitro} Tests".into()),
        ];
        assert_eq!(
            title(&chunks, true),
            "The role of <span class=\"nocase\"><i>in vivo</i></span> and <i>in vitro</i> tests"
        );
        assert_eq!(
            title(&[Chunk::Normal("Schlaf und Gedächtnis".into())], false),
            "Schlaf und Gedächtnis"
        );
    }

    #[test]
    fn markup_characters_in_titles_are_escaped() {
        assert_eq!(
            title(&[Chunk::Normal("Less <Than> & More".into())], false),
            "Less &lt;Than&gt; &amp; More"
        );
    }

    #[test]
    fn braced_names_are_detected_per_name() {
        assert_eq!(
            braced_names("{World Health Organization} and van der Berg, Jan AND {A} {B}"),
            [true, false, false]
        );
        assert_eq!(braced_names("Παπαδόπουλος, Νίκος and Ανδρέας"), [false, false]);
    }

    #[test]
    fn lenient_dates_report_what_was_dropped() {
        assert_eq!(
            lenient_date("2021-02-31"),
            Some((
                PartialDate {
                    year: 2021,
                    month: Some(2),
                    day: None
                },
                false
            ))
        );
        assert_eq!(lenient_date("2020-13-45x"), Some((PartialDate::year(2020), false)));
        assert_eq!(lenient_date("sometime"), None);
        assert_eq!(
            lenient_date("2024-02-29").map(|(d, ok)| (d.day, ok)),
            Some((Some(29), true))
        );
    }
}
