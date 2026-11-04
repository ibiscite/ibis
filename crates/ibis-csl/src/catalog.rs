//! The pinned CSL styles and locales. One compressed bundle sits in the binary
//! and only what a request names is unpacked, so the bundle costs disk, not
//! memory. `scripts/csl-bundle.mjs` writes it; a test checks every file's hash.
use crate::FormatError;
use serde::Deserialize;
use serde::de::IgnoredAny;
use std::collections::BTreeMap;
use std::io::Read;
use std::sync::{Arc, LazyLock, Mutex};

const BUNDLE: &[u8] = include_bytes!("../vendor/csl-bundle.bin");
const MAGIC: &[u8] = b"IBISCSL1";
/// The largest frame this format writes. The bundle is trusted; a bad build
/// still fails as `Runtime` instead of allocating what a corrupt header asks for.
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Which upstream release a part of the bundle was built from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Release {
    pub repo: String,
    pub tag: String,
    pub commit: String,
}

#[derive(Deserialize)]
struct Releases {
    styles: Release,
    locales: Release,
}

#[derive(Deserialize)]
struct Header {
    releases: Releases,
    blocks: Vec<(usize, usize, usize)>,
    locales: Vec<LocaleRow>,
    primary: BTreeMap<String, String>,
    renames: BTreeMap<String, String>,
    curated: Vec<(String, String)>,
}

#[derive(Deserialize)]
struct LocaleRow(String, String, String, usize, usize, usize);

#[derive(Deserialize)]
struct StyleRow {
    id: String,
    title: String,
    updated: String,
    parent: Option<String>,
    locale: Option<String>,
    format: Option<String>,
    fields: Vec<String>,
    block: usize,
    offset: usize,
    length: usize,
}

/// One entry in the style list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleEntry {
    pub id: String,
    pub title: String,
    /// The style's own last-changed date, `YYYY-MM-DD`: its only version marker.
    pub updated: String,
    /// The independent style this one borrows its rules from.
    pub parent: Option<String>,
    pub default_locale: Option<String>,
    pub format: Option<String>,
    pub fields: Vec<String>,
    pub curated: bool,
}

/// One language styles can be written in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleEntry {
    pub tag: String,
    pub native: String,
    pub english: String,
}

/// A page of the style list, with what the whole bundle holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StylePage {
    pub styles: Vec<StyleEntry>,
    pub total: usize,
    pub locales: Vec<LocaleEntry>,
    pub styles_release: Release,
    pub locales_release: Release,
}

/// A style ready for the processor: its own identity, and the rules to run.
#[derive(Debug)]
pub(crate) struct Resolved {
    /// The id asked for, after renames.
    pub id: String,
    /// The independent style whose XML this is.
    pub rules: String,
    pub xml: Arc<str>,
    pub traits: crate::input::StyleTraits,
    /// A dependent style's own default language, which replaces its parent's.
    pub forced_locale: Option<String>,
    /// What the output uses unless the request names a language.
    pub language: String,
    /// Its titles are written in sentence case, with capitalized subtitles.
    pub sentence_titles: bool,
    /// Author only, year only and narrative were checked against Zotero for
    /// these rules; elsewhere they are refused until each style's output is verified.
    pub verified_forms: bool,
}

static HEADER: LazyLock<Result<Header, FormatError>> = LazyLock::new(|| {
    let (header, _, _) = sections()?;
    serde_json::from_slice(&unpack(header)?).map_err(|_| FormatError::Runtime)
});
/// One warm style: a preview typed repeatedly never unpacks it twice.
static LAST: Mutex<Option<Arc<Resolved>>> = Mutex::new(None);

fn header() -> Result<&'static Header, FormatError> {
    HEADER.as_ref().map_err(|error| *error)
}

type Frames = (&'static [u8], &'static [u8], &'static [u8]);

fn sections() -> Result<Frames, FormatError> {
    let tail = BUNDLE.strip_prefix(MAGIC).ok_or(FormatError::Runtime)?;
    let (lengths, tail) = tail.split_at_checked(8).ok_or(FormatError::Runtime)?;
    let length = |range: std::ops::Range<usize>| {
        lengths
            .get(range)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
            .map(|length| length as usize)
            .ok_or(FormatError::Runtime)
    };
    let (header, tail) = tail.split_at_checked(length(0..4)?).ok_or(FormatError::Runtime)?;
    let (rows, blocks) = tail.split_at_checked(length(4..8)?).ok_or(FormatError::Runtime)?;
    Ok((header, rows, blocks))
}

fn unpack(frame: &[u8]) -> Result<Vec<u8>, FormatError> {
    let decoder = ruzstd::decoding::StreamingDecoder::new(frame).map_err(|_| FormatError::Runtime)?;
    let mut bytes = Vec::new();
    decoder
        .take(MAX_FRAME_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| FormatError::Runtime)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(FormatError::Runtime);
    }
    Ok(bytes)
}

fn file(block: usize, offset: usize, length: usize) -> Result<String, FormatError> {
    let (_, _, blocks) = sections()?;
    let &(start, compressed, _) = header()?.blocks.get(block).ok_or(FormatError::Runtime)?;
    let frame = blocks
        .get(start..start.saturating_add(compressed))
        .ok_or(FormatError::Runtime)?;
    let mut bytes = unpack(frame)?;
    let end = offset.checked_add(length).ok_or(FormatError::Runtime)?;
    if end > bytes.len() {
        return Err(FormatError::Runtime);
    }
    bytes.truncate(end);
    bytes.drain(..offset);
    String::from_utf8(bytes).map_err(|_| FormatError::Runtime)
}

fn rows() -> Result<Vec<StyleRow>, FormatError> {
    #[derive(Deserialize)]
    struct Row(
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Vec<String>,
        usize,
        usize,
        usize,
    );
    let (_, rows, _) = sections()?;
    let rows: Vec<Row> = serde_json::from_slice(&unpack(rows)?).map_err(|_| FormatError::Runtime)?;
    Ok(rows
        .into_iter()
        .map(|row| StyleRow {
            id: row.0,
            title: row.1,
            updated: row.2,
            parent: row.3,
            locale: row.4,
            format: row.5,
            fields: row.6,
            block: row.7,
            offset: row.8,
            length: row.9,
        })
        .collect())
}

fn renamed(id: &str) -> Result<&str, FormatError> {
    Ok(header()?.renames.get(id).map_or(id, String::as_str))
}

/// Zotero's list of styles that capitalize the first word of subtitles, read
/// from its `style.js`, for a style or its parent.
fn sentence_titles(id: &str) -> bool {
    let named = |name: &str| id == name || id.strip_prefix(name).is_some_and(|rest| rest.starts_with('-'));
    named("apa") || named("academy-of-management") || id.starts_with("freshwater-science")
}

/// What finding one style needs: borrowed, and skipping titles, so a lookup
/// costs about a megabyte instead of a copy of the whole list.
#[derive(Deserialize)]
struct Lookup<'a>(
    #[serde(borrow)] &'a str,
    IgnoredAny,
    IgnoredAny,
    #[serde(borrow)] Option<&'a str>,
    #[serde(borrow)] Option<&'a str>,
    IgnoredAny,
    IgnoredAny,
    usize,
    usize,
    usize,
);

/// A style by id, following its rename and its parent. Unknown is `UnsupportedStyle`.
pub(crate) fn resolve(id: &str) -> Result<Arc<Resolved>, FormatError> {
    let id = renamed(id)?;
    {
        let last = LAST.lock().map_err(|_| FormatError::Runtime)?;
        if let Some(style) = last.as_ref().filter(|style| style.id == id) {
            return Ok(style.clone());
        }
    }
    let (_, rows, _) = sections()?;
    let bytes = unpack(rows)?;
    let rows: Vec<Lookup<'_>> = serde_json::from_slice(&bytes).map_err(|_| FormatError::Runtime)?;
    let row = |id: &str| {
        rows.binary_search_by(|row| row.0.cmp(id))
            .map(|index| &rows[index])
            .map_err(|_| FormatError::UnsupportedStyle)
    };
    let own = row(id)?;
    let rules = own.3.map_or(Ok(own), row)?;
    let xml: Arc<str> = file(rules.7, rules.8, rules.9)?.into();
    let forced_locale = own.3.and(own.4).map(str::to_owned);
    let sentence_titles = sentence_titles(own.0) || sentence_titles(rules.0);
    let resolved = Arc::new(Resolved {
        id: own.0.to_owned(),
        rules: rules.0.to_owned(),
        traits: crate::input::style_traits(&xml),
        // A style may name a bare language ("fr"); report the dialect it means.
        language: forced_locale
            .as_deref()
            .or(rules.4)
            .map_or(Ok("en-US"), locale_tag)
            .unwrap_or("en-US")
            .to_owned(),
        forced_locale,
        sentence_titles,
        verified_forms: crate::STYLES.iter().any(|(id, _)| *id == rules.0),
        xml,
    });
    *LAST.lock().map_err(|_| FormatError::Runtime)? = Some(resolved.clone());
    Ok(resolved)
}

/// The bundled tag a request's language means: exact, or a language's main
/// dialect (`de` is `de-DE`). Anything else is refused, not approximated.
pub(crate) fn locale_tag(tag: &str) -> Result<&'static str, FormatError> {
    let header = header()?;
    let tag = header.primary.get(tag).map_or(tag, String::as_str);
    header
        .locales
        .iter()
        .find(|locale| locale.0 == tag)
        .map(|locale| locale.0.as_str())
        .ok_or(FormatError::InvalidLocale)
}

/// A locale file for the processor, or `None` if no such language is bundled.
/// The processor also asks for shortened tags of the language it runs in
/// (`sr-Latn` for `sr-Latn-RS`): those name the one bundled dialect they start,
/// or the language's main dialect.
pub(crate) fn locale_xml(tag: &str) -> Result<Option<String>, FormatError> {
    let header = header()?;
    let prefixed: Vec<_> = header
        .locales
        .iter()
        // The processor upper-cases the second subtag: it asks for `sr-CYRL`.
        .filter(|locale| {
            locale.0.len() > tag.len()
                && locale.0.as_bytes()[tag.len()] == b'-'
                && locale
                    .0
                    .get(..tag.len())
                    .is_some_and(|start| start.eq_ignore_ascii_case(tag))
        })
        .collect();
    let tag = match (locale_tag(tag), prefixed.as_slice()) {
        (Ok(tag), _) => tag,
        (Err(_), [only]) => only.0.as_str(),
        (Err(_), _) => match tag.split('-').next().map(locale_tag) {
            Some(Ok(main)) => main,
            _ => return Ok(None),
        },
    };
    let row = header.locales.iter().find(|locale| locale.0 == tag);
    row.map(|locale| file(locale.3, locale.4, locale.5)).transpose()
}

/// The common styles, most used first: each one's id and title.
pub fn curated() -> Result<&'static [(String, String)], FormatError> {
    Ok(&header()?.curated)
}

/// Styles matching every word, most useful first, then by id. An empty query
/// gives the curated styles, then the rest.
pub fn styles(query: &str, limit: usize, offset: usize) -> Result<StylePage, FormatError> {
    let header = header()?;
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let curated: BTreeMap<&str, (usize, &str)> = header
        .curated
        .iter()
        .enumerate()
        .map(|(rank, (id, title))| (id.as_str(), (rank, title.as_str())))
        .collect();
    let mut found: Vec<(usize, StyleRow)> = rows()?
        .into_iter()
        .filter_map(|row| {
            let id = row.id.to_lowercase();
            let title = curated
                .get(row.id.as_str())
                .map_or(row.title.as_str(), |(_, title)| title);
            let title = title.to_lowercase();
            let original = row.title.to_lowercase();
            words
                .iter()
                .all(|word| id.contains(word) || title.contains(word) || original.contains(word))
                .then(|| {
                    let phrase = words.join(" ");
                    let rank = match curated.get(row.id.as_str()) {
                        Some((rank, _)) => *rank,
                        None if !words.is_empty() && (id == phrase || title == phrase) => usize::MAX - 3,
                        None if !words.is_empty() && (id.starts_with(&phrase) || title.starts_with(&phrase)) => {
                            usize::MAX - 2
                        }
                        None if row.parent.is_none() => usize::MAX - 1,
                        None => usize::MAX,
                    };
                    (rank, row)
                })
        })
        .collect();
    found.sort_by(|(left_rank, left), (right_rank, right)| left_rank.cmp(right_rank).then(left.id.cmp(&right.id)));
    let total = found.len();
    let styles = found
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(_, row)| {
            let curated_title = curated.get(row.id.as_str()).map(|(_, title)| (*title).to_owned());
            StyleEntry {
                curated: curated_title.is_some(),
                title: curated_title.unwrap_or(row.title),
                id: row.id,
                updated: row.updated,
                parent: row.parent,
                default_locale: row.locale,
                format: row.format,
                fields: row.fields,
            }
        })
        .collect();
    Ok(StylePage {
        styles,
        total,
        locales: header
            .locales
            .iter()
            .map(|locale| LocaleEntry {
                tag: locale.0.clone(),
                native: locale.1.clone(),
                english: locale.2.clone(),
            })
            .collect(),
        styles_release: header.releases.styles.clone(),
        locales_release: header.releases.locales.clone(),
    })
}

/// Every file with its manifest path, for the verification test and the sweep.
pub fn files() -> Result<Vec<(String, String)>, FormatError> {
    let mut cache: Option<(usize, Vec<u8>)> = None;
    let mut files = Vec::new();
    let mut read = |path: String, block: usize, offset: usize, length: usize| {
        if cache.as_ref().is_none_or(|(held, _)| *held != block) {
            let (_, _, blocks) = sections()?;
            let &(start, compressed, _) = header()?.blocks.get(block).ok_or(FormatError::Runtime)?;
            let frame = blocks
                .get(start..start.saturating_add(compressed))
                .ok_or(FormatError::Runtime)?;
            cache = Some((block, unpack(frame)?));
        }
        let bytes = cache.as_ref().map(|(_, bytes)| bytes).ok_or(FormatError::Runtime)?;
        let bytes = bytes
            .get(offset..offset.saturating_add(length))
            .ok_or(FormatError::Runtime)?;
        files.push((
            path,
            String::from_utf8(bytes.to_vec()).map_err(|_| FormatError::Runtime)?,
        ));
        Ok::<_, FormatError>(())
    };
    for row in rows()? {
        let folder = if row.parent.is_some() { "dependent/" } else { "" };
        read(
            format!("styles/{folder}{}.csl", row.id),
            row.block,
            row.offset,
            row.length,
        )?;
    }
    for locale in &header()?.locales {
        read(
            format!("locales/locales-{}.xml", locale.0),
            locale.3,
            locale.4,
            locale.5,
        )?;
    }
    Ok(files)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_dependent_style_runs_its_parents_rules_in_its_own_language() {
        // An invented id can't name a real journal's rule, so find one in the data.
        let german = rows()
            .unwrap()
            .into_iter()
            .find(|row| row.parent.is_some() && row.locale.as_deref().is_some_and(|locale| locale.starts_with("de")))
            .unwrap();
        let resolved = resolve(&german.id).unwrap();
        assert_eq!(Some(&resolved.rules), german.parent.as_ref());
        assert_eq!(Some(resolved.language.as_str()), german.locale.as_deref());
        assert_eq!(resolved.forced_locale, german.locale);
        assert!(resolved.xml.contains("<citation"));
    }

    #[test]
    fn an_old_id_finds_its_new_style_and_an_unknown_id_is_refused() {
        let (old, new) = header().unwrap().renames.iter().next().unwrap();
        assert_eq!(&resolve(old).unwrap().id, new);
        assert_eq!(resolve("invented-style").err(), Some(FormatError::UnsupportedStyle));
    }

    #[test]
    fn a_language_is_a_bundled_tag_or_its_main_dialect_never_a_guess() {
        assert_eq!(locale_tag("de-AT"), Ok("de-AT"));
        assert_eq!(locale_tag("de"), Ok("de-DE"));
        assert_eq!(locale_tag("en"), Ok("en-US"));
        assert_eq!(locale_tag("xx-INVENTED"), Err(FormatError::InvalidLocale));
        assert!(locale_xml("fr-FR").unwrap().unwrap().contains("xml:lang=\"fr-FR\""));
        assert_eq!(locale_xml("xx-INVENTED"), Ok(None));
    }

    #[test]
    fn a_damaged_frame_is_a_formatter_failure_never_text() {
        assert_eq!(unpack(b"not a zstd frame"), Err(FormatError::Runtime));
        assert_eq!(unpack(&[]), Err(FormatError::Runtime));
        assert_eq!(file(usize::MAX, 0, 1), Err(FormatError::Runtime));
        assert_eq!(file(0, usize::MAX, 2), Err(FormatError::Runtime));
    }

    #[test]
    fn the_last_style_kept_never_answers_for_another() {
        let apa = resolve("apa").unwrap();
        let nature = resolve("nature").unwrap();
        assert_ne!(apa.xml, nature.xml);
        assert_eq!(resolve("apa").unwrap().xml, apa.xml);
    }

    #[test]
    fn every_listed_language_has_a_file_the_processor_can_find() {
        for locale in &header().unwrap().locales {
            assert!(locale_xml(&locale.0).unwrap().is_some(), "{}", locale.0);
        }
        // What the processor asks for while running sr-Latn-RS and sr-Cyrl-RS.
        for (asked, found) in [
            ("sr-LATN", "sr-Latn-RS"),
            ("sr-CYRL", "sr-Cyrl-RS"),
            ("sr-RS", "sr-Latn-RS"),
        ] {
            assert!(locale_xml(asked).unwrap().unwrap().contains(found), "{asked}");
        }
        assert_eq!(locale_xml("xx-INVENTED"), Ok(None));
    }

    #[test]
    fn listing_puts_curated_styles_first_and_finds_by_title_or_id() {
        let page = styles("", 3, 0).unwrap();
        assert!(page.total > 10_000);
        assert_eq!(page.styles[0].id, "apa");
        assert!(page.styles.iter().all(|style| style.curated));
        assert!(page.locales.iter().any(|locale| locale.tag == "zh-CN"));
        let found = styles("7714 2025", 10, 0).unwrap();
        assert_eq!(found.total, 3);
        assert!(found.styles.iter().all(|style| style.curated));
        assert_eq!(
            styles("", 2, 1).unwrap().styles[0].id,
            styles("", 3, 0).unwrap().styles[1].id
        );
        assert_eq!(styles("invented-nothing-here", 10, 0).unwrap().total, 0);
    }
}
