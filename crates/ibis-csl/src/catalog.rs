//! The pinned CSL styles and locales. One compressed bundle sits in the binary
//! and only what a request names is unpacked, so the bundle costs disk, not
//! memory. `scripts/csl-bundle.mjs` writes it; a test checks every file's hash.
use crate::FormatError;
use schemars::JsonSchema;
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::sync::{Arc, LazyLock, Mutex};

const BUNDLE: &[u8] = include_bytes!("../vendor/csl-bundle.bin");
const MAGIC: &[u8] = b"IBISCSL1";
/// The largest frame this format writes. The bundle is trusted; a bad build
/// still fails as `Runtime` instead of allocating what a corrupt header asks for.
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Which upstream release a part of the bundle was built from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
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
    curated: Vec<Curated>,
}

/// A common style: its id, the style's full title, and its one short name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Curated(pub String, pub String, pub String);
impl Curated {
    pub fn id(&self) -> &str {
        &self.0
    }
    /// The full title, as the style list shows it.
    pub fn title(&self) -> &str {
        &self.1
    }
    /// The short name every front end shows.
    pub fn name(&self) -> &str {
        &self.2
    }
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct StyleEntry {
    pub id: String,
    pub title: String,
    /// The style's own last-changed date, `YYYY-MM-DD`: its only version marker.
    pub updated: String,
    /// The independent style whose rules this one uses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The language this style is written in unless a request names another.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_locale: Option<String>,
    /// CSL's citation format, such as `author-date`, `numeric` or `note`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    /// One of the styles offered first.
    pub curated: bool,
    /// Yours: kept or added, so it wins over the bundled style of its id and
    /// never changes with an Ibis release.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub yours: bool,
    /// For a style of yours, the bundled style's date when it differs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundled_updated: Option<String>,
}

/// What keeping a style did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Kept {
    /// The style kept, under its current id.
    pub id: String,
    /// You already kept this version, so nothing changed.
    pub already: bool,
    /// Its parent, kept too because you had no parent of yours by that id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

/// What removing a style of yours did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Removed {
    pub id: String,
    /// You had a style by that id.
    pub removed: bool,
    /// Your styles that use its rules, which now use the bundled style's or are refused.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependents: Vec<String>,
}

/// Someone a style names as its author or contributor. Their email stays in the
/// file: a credit needs only the name and the page they chose to give.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Person {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

/// What a style's own `<info>` says about who made it and on what terms,
/// for the "About citation styles" view. A dependent style's are its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct StyleCredits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub authors: Vec<Person>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub contributors: Vec<Person>,
    /// The licence sentence and the address of the licence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rights: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Web pages documenting the rules the style follows.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub documentation: Vec<String>,
}

/// One language styles can be written in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
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
    /// What the processor keeps its warm engine under: the rules' id for the
    /// bundle's, their contents for yours, so a changed file is never stale.
    pub engine_key: String,
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
/// One warm style: a preview typed repeatedly never unpacks it twice. It holds
/// what it was built from, your files included, and answers only while those
/// are the same.
static LAST: Mutex<Option<Last>> = Mutex::new(None);

struct Last {
    asked: String,
    own: Option<String>,
    parent: Option<String>,
    parent_own: Option<String>,
    resolved: Arc<Resolved>,
}

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

fn sha256(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A style by id: yours first, then upstream's renames (yours again under the
/// new id), then the bundle, following its parent the same way. Unknown is
/// `UnsupportedStyle`; a style of yours that cannot be used is `InvalidStyle`,
/// never answered by the bundled style of the same id.
pub(crate) fn resolve(asked: &str) -> Result<Arc<Resolved>, FormatError> {
    let (id, own) = match crate::yours::read(asked)? {
        Some(xml) => (asked, Some(xml)),
        None => {
            let id = renamed(asked)?;
            (id, crate::yours::read(id)?)
        }
    };
    {
        let last = LAST.lock().map_err(|_| FormatError::Runtime)?;
        if let Some(last) = last.as_ref().filter(|last| last.asked == asked && last.own == own) {
            let parent_own = last.parent.as_deref().map(crate::yours::read).transpose()?.flatten();
            if parent_own == last.parent_own {
                return Ok(last.resolved.clone());
            }
        }
    }
    let (resolved, parent, parent_own) = build(id, own.as_deref())?;
    let resolved = Arc::new(resolved);
    *LAST.lock().map_err(|_| FormatError::Runtime)? = Some(Last {
        asked: asked.to_owned(),
        own,
        parent,
        parent_own,
        resolved: resolved.clone(),
    });
    Ok(resolved)
}

/// The style `id` from `own`, a file of yours, or from the bundle; with the
/// parent it named and your file for that parent, which the cache compares.
fn build(id: &str, own: Option<&str>) -> Result<(Resolved, Option<String>, Option<String>), FormatError> {
    let (_, rows, _) = sections()?;
    let bytes = unpack(rows)?;
    let rows: Vec<Lookup<'_>> = serde_json::from_slice(&bytes).map_err(|_| FormatError::Runtime)?;
    let row = |id: &str| {
        rows.binary_search_by(|row| row.0.cmp(id))
            .map(|index| &rows[index])
            .map_err(|_| FormatError::UnsupportedStyle)
    };
    // This style's own text, parent and language, from your file or the bundle.
    let (parent, own_locale) = match own {
        Some(xml) => {
            let meta = crate::info::Metadata::read(xml);
            if meta.id.as_deref() != Some(id) || meta.title.is_none() {
                return Err(FormatError::InvalidStyle);
            }
            (meta.parent, meta.default_locale)
        }
        None => {
            let own = row(id)?;
            (own.3.map(str::to_owned), own.4.map(str::to_owned))
        }
    };
    // The rules to run: this style's, or its parent's (yours first), which must
    // be an independent style other than this one.
    let parent_own = parent.as_deref().map(crate::yours::read).transpose()?.flatten();
    let (rules, xml, rules_locale, rules_yours): (String, Arc<str>, Option<String>, bool) = match (&parent, &parent_own)
    {
        (None, _) => match own {
            Some(xml) => (id.to_owned(), xml.into(), own_locale.clone(), true),
            None => {
                let own = row(id)?;
                (
                    id.to_owned(),
                    file(own.7, own.8, own.9)?.into(),
                    own_locale.clone(),
                    false,
                )
            }
        },
        (Some(parent), _) if parent == id => return Err(FormatError::InvalidStyle),
        (Some(parent), Some(xml)) => {
            let meta = crate::info::Metadata::read(xml);
            if meta.parent.is_some() || meta.id.as_deref() != Some(parent.as_str()) {
                return Err(FormatError::InvalidStyle);
            }
            (parent.clone(), xml.as_str().into(), meta.default_locale, true)
        }
        (Some(parent), None) => {
            let found = row(parent).map_err(|_| match own {
                Some(_) => FormatError::InvalidStyle,
                None => FormatError::Runtime,
            })?;
            if found.3.is_some() {
                return Err(FormatError::InvalidStyle);
            }
            (
                parent.clone(),
                file(found.7, found.8, found.9)?.into(),
                found.4.map(str::to_owned),
                false,
            )
        }
    };
    let forced_locale = parent.as_ref().and(own_locale);
    // A style may name a bare language ("fr"); report the dialect it means. A
    // language of yours that Ibis does not have is refused, never approximated.
    let language = match forced_locale.as_deref().or(rules_locale.as_deref()) {
        None => "en-US",
        Some(tag) if own.is_some() || rules_yours => locale_tag(tag).map_err(|_| FormatError::InvalidStyle)?,
        Some(tag) => locale_tag(tag).unwrap_or("en-US"),
    }
    .to_owned();
    let hash = sha256(&xml);
    let resolved = Resolved {
        engine_key: if rules_yours {
            format!("yours:{hash}")
        } else {
            rules.clone()
        },
        sentence_titles: sentence_titles(id) || sentence_titles(&rules),
        verified_forms: crate::VERIFIED.contains(&hash.as_str()),
        traits: crate::input::style_traits(&xml),
        language,
        forced_locale,
        xml,
    };
    Ok((resolved, parent, parent_own))
}

/// A style you are adding, checked as it would be used, before it is saved.
pub(crate) fn candidate(xml: &str) -> Result<(String, Resolved), FormatError> {
    if xml.len() > crate::yours::MAX_BYTES {
        return Err(FormatError::InvalidStyle);
    }
    let id = crate::info::Metadata::read(xml)
        .id
        .filter(|id| crate::yours::valid_id(id))
        .ok_or(FormatError::InvalidStyle)?;
    let (resolved, _, _) = build(&id, Some(xml))?;
    Ok((id, resolved))
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

/// The common styles, most used first.
pub fn curated() -> Result<&'static [Curated], FormatError> {
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
        .map(|(rank, style)| (style.id(), (rank, style.title())))
        .collect();
    let rows = rows()?;
    let yours = your_entries(&rows)?;
    let shadowed: Vec<String> = yours.iter().map(|style| style.id.clone()).collect();
    let entries = yours.into_iter().chain(
        rows.into_iter()
            .filter(|row| !shadowed.contains(&row.id))
            .map(|row| entry(row, &curated)),
    );
    let phrase = words.join(" ");
    let mut found: Vec<(usize, StyleEntry)> = entries
        .filter_map(|style| {
            let id = style.id.to_lowercase();
            let title = style.title.to_lowercase();
            words
                .iter()
                .all(|word| id.contains(word) || title.contains(word))
                .then(|| {
                    // Yours first, then the common styles, then the closest matches.
                    let rank = match curated.get(style.id.as_str()) {
                        _ if style.yours => 0,
                        Some((rank, _)) => rank + 1,
                        None if !words.is_empty() && (id == phrase || title == phrase) => usize::MAX - 3,
                        None if !words.is_empty() && (id.starts_with(&phrase) || title.starts_with(&phrase)) => {
                            usize::MAX - 2
                        }
                        None if style.parent.is_none() => usize::MAX - 1,
                        None => usize::MAX,
                    };
                    (rank, style)
                })
        })
        .collect();
    found.sort_by(|(left_rank, left), (right_rank, right)| left_rank.cmp(right_rank).then(left.id.cmp(&right.id)));
    let total = found.len();
    let styles = found
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(_, style)| style)
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

fn entry(row: StyleRow, curated: &BTreeMap<&str, (usize, &str)>) -> StyleEntry {
    // The curated title also matches what the style calls itself.
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
        yours: false,
        bundled_updated: None,
    }
}

/// A word that names a category or language: anything else in your file is left out.
fn plain(value: String) -> Option<String> {
    (value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
    .then_some(value)
}

/// Your styles as the list shows them. A file that does not describe itself
/// as the style its name says is left out of the list; using it is refused.
fn your_entries(rows: &[StyleRow]) -> Result<Vec<StyleEntry>, FormatError> {
    let curated = curated()?;
    Ok(crate::yours::ids()?
        .into_iter()
        .filter_map(|id| {
            let xml = crate::yours::read(&id).ok()??;
            let meta = crate::info::Metadata::read(&xml);
            if meta.id.as_deref() != Some(id.as_str()) {
                return None;
            }
            let updated = meta.updated.unwrap_or_default();
            let bundled = rows.iter().find(|row| row.id == id);
            Some(StyleEntry {
                curated: curated.iter().any(|style| style.id() == id),
                title: meta.title?,
                bundled_updated: bundled.map(|row| row.updated.clone()).filter(|date| *date != updated),
                updated,
                parent: meta.parent.and_then(plain),
                default_locale: meta.default_locale.and_then(plain),
                format: meta.format.and_then(plain),
                fields: meta.fields.into_iter().filter_map(plain).collect(),
                yours: true,
                id,
            })
        })
        .collect())
}

/// Keep the bundled style `id` as it is now: a copy, byte for byte, in your
/// styles, which no later Ibis release changes. A dependent style's parent is
/// kept too unless you have one, since its rules are the parent's.
pub fn keep_style(id: &str) -> Result<Kept, FormatError> {
    let id = renamed(id)?;
    let rows = rows()?;
    let find = |id: &str| {
        rows.iter()
            .find(|row| row.id == id)
            .ok_or(FormatError::UnsupportedStyle)
    };
    let own = find(id)?;
    let xml = file(own.block, own.offset, own.length)?;
    let already = match crate::yours::read(id) {
        Ok(None) => false,
        Ok(Some(existing)) if existing == xml => true,
        _ => return Err(FormatError::KeptDifferent),
    };
    let parent = match &own.parent {
        Some(parent) if crate::yours::read(parent)?.is_none() => {
            let row = find(parent)?;
            crate::yours::write(parent, &file(row.block, row.offset, row.length)?)?;
            Some(parent.clone())
        }
        _ => None,
    };
    if !already {
        crate::yours::write(id, &xml)?;
    }
    Ok(Kept {
        id: id.to_owned(),
        already,
        parent,
    })
}

/// Remove your style `id`, so the bundled style of that id, if any, applies again.
pub fn remove_style(id: &str) -> Result<Removed, FormatError> {
    let removed = crate::yours::valid_id(id) && crate::yours::remove(id)?;
    let dependents = crate::yours::ids()?
        .into_iter()
        .filter(|other| {
            crate::yours::read(other)
                .ok()
                .flatten()
                .is_some_and(|xml| crate::info::Metadata::read(&xml).parent.as_deref() == Some(id))
        })
        .collect();
    Ok(Removed {
        id: id.to_owned(),
        removed,
        dependents,
    })
}

/// One style's own entry and credits, after renames; a dependent style's are its
/// own, not its parent's. Unknown is `UnsupportedStyle`.
pub fn style_credits(id: &str) -> Result<(StyleEntry, StyleCredits), FormatError> {
    let rows = rows()?;
    let yours = your_entries(&rows)?;
    let id = match crate::yours::read(id)? {
        Some(_) => id,
        None => renamed(id)?,
    };
    let (style, xml) = match yours.into_iter().find(|style| style.id == id) {
        Some(style) => {
            let xml = crate::yours::read(id)?.ok_or(FormatError::InvalidStyle)?;
            (style, xml)
        }
        None if crate::yours::read(id)?.is_some() => return Err(FormatError::InvalidStyle),
        None => {
            let curated = header()?
                .curated
                .iter()
                .map(|style| (style.id(), (0, style.title())))
                .collect();
            let row = rows
                .into_iter()
                .find(|row| row.id == id)
                .ok_or(FormatError::UnsupportedStyle)?;
            let xml = file(row.block, row.offset, row.length)?;
            (entry(row, &curated), xml)
        }
    };
    let info = crate::info::Info::read(&xml);
    Ok((
        style,
        StyleCredits {
            summary: info.summary,
            authors: info.authors,
            contributors: info.contributors,
            rights: info.rights,
            license: info.license,
            documentation: info.documentation,
        },
    ))
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
        assert_eq!(resolved.xml, resolve(german.parent.as_deref().unwrap()).unwrap().xml);
        assert_eq!(Some(resolved.language.as_str()), german.locale.as_deref());
        assert_eq!(resolved.forced_locale, german.locale);
        assert!(resolved.xml.contains("<citation"));
    }

    #[test]
    fn an_old_id_finds_its_new_style_and_an_unknown_id_is_refused() {
        let (old, new) = header().unwrap().renames.iter().next().unwrap();
        assert_eq!(resolve(old).unwrap().xml, resolve(new).unwrap().xml);
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
    fn the_styles_checked_against_zotero_are_known_by_their_contents() {
        // A new upstream version of one needs its forms checked again, and its
        // SHA-256 added to `VERIFIED`, before it offers them.
        for (id, _) in crate::STYLES {
            assert!(resolve(id).unwrap().verified_forms, "{id}");
        }
        assert!(!resolve("nature").unwrap().verified_forms);
    }

    #[test]
    fn credits_name_a_styles_own_authors_and_licence_and_follow_renames() {
        let (style, apa) = style_credits("apa").unwrap();
        assert_eq!(style.id, "apa");
        assert!(style.curated);
        assert!(!apa.authors.is_empty());
        assert!(
            apa.authors
                .iter()
                .all(|person| !person.name.is_empty() && !person.name.contains('@'))
        );
        assert_eq!(
            apa.license.as_deref(),
            Some("http://creativecommons.org/licenses/by-sa/3.0/")
        );
        // A dependent style credits itself and points at the rules it borrows.
        let dependent = rows().unwrap().into_iter().find(|row| row.parent.is_some()).unwrap();
        let (style, _) = style_credits(&dependent.id).unwrap();
        assert_eq!(style.parent, dependent.parent);
        assert_eq!(style.updated, dependent.updated);
        let (old, new) = header().unwrap().renames.iter().next().unwrap();
        assert_eq!(&style_credits(old).unwrap().0.id, new);
        assert_eq!(
            style_credits("invented-style").err(),
            Some(FormatError::UnsupportedStyle)
        );
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
