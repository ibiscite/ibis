//! Instant keyword search with an explanation for every match.
//!
//! Each record's searchable text is normalised once, when an index is built,
//! into one compact buffer, so a keystroke only scans it. Matching ignores
//! case and accents ("garcia" finds "García"), works for any script, including
//! CJK, and takes words in any order. Every query term must match some field,
//! and the best match of each term counts: a whole word, then the start of a
//! word, then inside a word (terms of three or more characters, or CJK), then
//! one typo at the start of a word (words of five or more Latin, Greek or
//! Cyrillic letters, first letter right). `a:`, `t:`, `j:`, `k:`, `#` and `y:`
//! limit a term to names, title, journal, key, tags or years. "et al." and
//! "and" are ignored. An exact citation key always matches and ranks first,
//! then an exact title.

use std::collections::BTreeMap;

use memchr::memmem::Finder;
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::identifier::{self, Identifier};
use crate::model::{Name, Reference, is_cjk_char, strip_markup};

mod pasted;
mod snapshot;
pub use snapshot::{PastedList, RecordHit, SnapshotIndex};

/// Which part of a record matched a query term.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "type-export", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum MatchField {
    Citekey,
    Title,
    Author,
    Year,
    Doi,
    Keyword,
    Container,
    Abstract,
    /// An ISBN, PubMed ID or arXiv ID given as the whole query.
    Identifier,
}

impl MatchField {
    const ALL: [MatchField; 8] = [
        MatchField::Citekey,
        MatchField::Title,
        MatchField::Author,
        MatchField::Year,
        MatchField::Doi,
        MatchField::Keyword,
        MatchField::Container,
        MatchField::Abstract,
    ];

    fn weight(self) -> u32 {
        match self {
            MatchField::Citekey => 12,
            MatchField::Title => 10,
            MatchField::Author => 9,
            MatchField::Doi => 9,
            MatchField::Year => 6,
            MatchField::Keyword => 5,
            MatchField::Container => 3,
            MatchField::Abstract => 1,
            // Identifier hits are scored apart, in `by_identifier`.
            MatchField::Identifier => 0,
        }
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// How well a term matched, multiplied by the field's weight.
const WHOLE_WORD: u32 = 4;
const WORD_START: u32 = 3;
const INSIDE_WORD: u32 = 2;
const ONE_TYPO: u32 = 1;

/// Fields a plain term may match, and those a typo may match (titles, people
/// and journals): keys, identifiers, years and tags are too exact to guess
/// at, abstracts too long.
const ANY_FIELD: u8 = u8::MAX;
/// Longer words and queries are pasted, not typed, and get no typo allowance.
const MAX_TYPO_TERM: usize = 32;
const MAX_TYPO_QUERY_TERMS: usize = 8;
const TYPO_FIELDS: u8 = 0b0100_0110;

/// One search result.
#[derive(Debug, Clone, Serialize)]
pub struct Hit<'a, T = Reference> {
    pub reference: &'a T,
    pub score: u32,
    /// Fields that matched, strongest first, so the UI can say why.
    pub matched: Vec<MatchField>,
    /// Some term matched only with one typo, so the UI can say "close match".
    pub typo: bool,
}

/// Metadata needed by the shared scorer, independent of source/model shape.
trait SearchDocument {
    type Identity: Ord;
    type Source: PartialEq;
    fn identity(&self) -> &Self::Identity;
    fn source(&self) -> &Self::Source;
    fn year(&self) -> Option<i32>;
    fn exact_key(&self, query: &str) -> bool;
}

fn key_eq(key: &str, query: &str) -> bool {
    key.chars().flat_map(lower).eq(query.chars())
}
fn lower(c: char) -> impl Iterator<Item = char> {
    c.to_lowercase().map(|c| if c == 'ς' { 'σ' } else { c })
}
impl SearchDocument for Reference {
    type Identity = crate::model::RecordId;
    type Source = String;
    fn identity(&self) -> &Self::Identity {
        &self.id
    }
    fn source(&self) -> &Self::Source {
        &self.id.source
    }
    fn year(&self) -> Option<i32> {
        Reference::year(self)
    }
    fn exact_key(&self, query: &str) -> bool {
        self.citekey.as_deref().is_some_and(|key| key_eq(key, query))
    }
}

/// The normalised text of every record of an index in two buffers, abstracts
/// apart because they are searched last. One scan finds a term in every
/// record, and the index holds two allocations instead of eight per record.
#[derive(Default)]
struct Texts {
    meta: String,
    abstracts: String,
    records: Vec<Bounds>,
}

#[derive(Clone, Copy)]
struct Bounds {
    /// Where each field but the abstract ends in `meta`, in [`MatchField::ALL`] order.
    ends: [u32; 7],
    /// Where the title proper ends; a short title may follow it.
    title_end: u32,
    abstract_end: u32,
}

impl Texts {
    /// Add a record's fields in [`MatchField::ALL`] order. `title_short` and
    /// `container_short` (a journal abbreviation) are added to the title and
    /// journal unless they are already part of them. Text past 4 GiB in either
    /// buffer is left out, which no real library comes near.
    fn push(&mut self, mut fields: [String; 8], title_short: &str, container_short: &str) {
        let title_len = fields[1].len();
        for (field, short) in [(1, title_short), (6, container_short)] {
            let short = normalize(short);
            if !short.is_empty() && !format!(" {} ", fields[field]).contains(&format!(" {short} ")) {
                push_words(&mut fields[field], &short);
            }
        }
        // A space before every field makes word starts and ends checkable
        // from the buffer alone.
        let push = |buffer: &mut String, field: &str| {
            if u32::try_from(buffer.len() + field.len() + 1).is_ok() {
                buffer.push(' ');
                buffer.push_str(field);
            }
            buffer.len() as u32
        };
        let mut ends = [0; 7];
        for (end, field) in ends.iter_mut().zip(&fields) {
            *end = push(&mut self.meta, field);
        }
        let title_start = ends[0] as usize + 1;
        let title_end = (title_start + title_len).min(ends[1] as usize) as u32;
        push(&mut self.abstracts, &fields[7]);
        self.records.push(Bounds {
            ends,
            title_end,
            abstract_end: self.abstracts.len() as u32,
        });
    }

    fn shrink(mut self) -> Self {
        self.meta.shrink_to_fit();
        self.abstracts.shrink_to_fit();
        self.records.shrink_to_fit();
        self
    }

    fn heap_bytes(&self) -> usize {
        self.meta.capacity() + self.abstracts.capacity() + self.records.capacity() * std::mem::size_of::<Bounds>()
    }

    /// Where record `n` starts in `meta` and in `abstracts`.
    fn starts(&self, n: usize) -> (usize, usize) {
        n.checked_sub(1).map_or((0, 0), |n| {
            (self.records[n].ends[6] as usize, self.records[n].abstract_end as usize)
        })
    }

    /// The span of field `bit` of record `n` in its buffer, after its space.
    fn span(&self, n: usize, bit: usize) -> (usize, usize) {
        let bounds = &self.records[n];
        let (meta, abstracts) = self.starts(n);
        let (before, end) = match bit {
            7 => (abstracts, bounds.abstract_end as usize),
            0 => (meta, bounds.ends[0] as usize),
            _ => (bounds.ends[bit - 1] as usize, bounds.ends[bit] as usize),
        };
        ((before + 1).min(end), end)
    }

    fn field(&self, n: usize, bit: usize) -> &str {
        let (start, end) = self.span(n, bit);
        if bit == 7 {
            &self.abstracts[start..end]
        } else {
            &self.meta[start..end]
        }
    }

    fn title_proper(&self, n: usize) -> &str {
        let (start, _) = self.span(n, 1);
        &self.meta[start..(self.records[n].title_end as usize).max(start)]
    }
}

fn push_words(text: &mut String, words: &str) {
    if !text.is_empty() && !words.is_empty() {
        text.push(' ');
    }
    text.push_str(words);
}

/// Every name, normalised, followed by its given-name initials when there
/// are several ("jrr" for J. R. R. or John Ronald Reuel), so `jrr tolkien`
/// finds it as well as `j tolkien`.
fn name_text<'a>(names: impl Iterator<Item = &'a Name>) -> String {
    let mut text = String::new();
    for name in names {
        let display = normalize(&name.display());
        push_words(&mut text, &display);
        // "山田 太郎" is also written without the space.
        if display.contains(' ') && display.chars().any(is_cjk_char) {
            push_words(&mut text, &display.replace(' ', ""));
        }
        let given = normalize(name.given.as_deref().unwrap_or(""));
        let initials: String = given.split(' ').filter_map(|word| word.chars().next()).collect();
        if initials.chars().count() > 1 && !initials.chars().any(is_cjk_char) {
            push_words(&mut text, &initials);
        }
    }
    text
}

/// Search index that owns its references, for front ends that keep a library
/// open and search it many times. Normalization happens once, when it is built.
#[derive(Default)]
pub struct OwnedIndex {
    references: Vec<Reference>,
    texts: Texts,
    identifiers: Vec<Vec<Identifier>>,
    source_counts: BTreeMap<String, usize>,
}

impl OwnedIndex {
    pub fn new(references: Vec<Reference>) -> Self {
        let mut texts = Texts::default();
        for reference in &references {
            prepare(reference, &mut texts);
        }
        let mut source_counts = BTreeMap::new();
        for reference in &references {
            *source_counts.entry(reference.id.source.clone()).or_default() += 1;
        }
        let identifiers = references.iter().map(identifier::of_reference).collect();
        OwnedIndex {
            references,
            texts: texts.shrink(),
            identifiers,
            source_counts,
        }
    }

    /// Counts by unchanged record source ID, computed once with the snapshot.
    /// Sources with no records have no entry.
    pub fn source_counts(&self) -> &BTreeMap<String, usize> {
        &self.source_counts
    }

    pub fn references(&self) -> &[Reference] {
        &self.references
    }

    /// Return up to `limit` matches, best first. An empty query matches nothing.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit<'_>> {
        self.search_until(query, limit, &|| false).unwrap_or_default()
    }

    /// Like [`OwnedIndex::search`], but gives up and returns `None` as soon as
    /// `stop` returns true, so a search for an outdated query does not keep
    /// the caller waiting. `stop` is checked during record and term processing.
    pub fn search_until(&self, query: &str, limit: usize, stop: &dyn Fn() -> bool) -> Option<Vec<Hit<'_>>> {
        self.search_scoped_until(query, limit, None, stop)
    }

    /// Search only these exact source IDs before ranking or limiting results.
    /// `None` searches all sources; an empty slice searches none.
    pub fn search_scoped(&self, query: &str, limit: usize, scope: Option<&[String]>) -> Vec<Hit<'_>> {
        self.search_scoped_until(query, limit, scope, &|| false)
            .unwrap_or_default()
    }

    /// Scoped search with the same cancellation contract as [`Self::search_until`].
    pub fn search_scoped_until(
        &self,
        query: &str,
        limit: usize,
        scope: Option<&[String]>,
        stop: &dyn Fn() -> bool,
    ) -> Option<Vec<Hit<'_>>> {
        let ids = self.references.iter().zip(self.identifiers.iter().map(Vec::as_slice));
        if let Some(hits) = by_identifier(ids, query, limit, scope, stop)? {
            return Some(hits);
        }
        rank(&self.references, &self.texts, query, limit, scope, stop)
    }
}

fn prepare(r: &Reference, texts: &mut Texts) {
    let variable = |name: &str| {
        r.variables
            .get(name)
            .map(|value| strip_markup(value))
            .unwrap_or_default()
    };
    texts.push(
        [
            normalize(r.citekey.as_deref().unwrap_or("")),
            normalize(&r.title_plain().unwrap_or_default()),
            name_text(
                r.authors
                    .iter()
                    .chain(&r.editors)
                    .chain(r.other_names.values().flatten()),
            ),
            r.year().map(|y| y.to_string()).unwrap_or_default(),
            normalize(r.doi.as_deref().unwrap_or("")),
            normalize(&r.keywords.join(" ")),
            normalize(&r.container_title.as_deref().map(strip_markup).unwrap_or_default()),
            normalize(r.abstract_text.as_deref().unwrap_or("")),
        ],
        &variable("title-short"),
        &variable("container-title-short"),
    );
}

/// When the whole query is a DOI, ISBN, PubMed ID or arXiv ID that some
/// records in scope carry, those records, newest first. `Some(None)` falls
/// back to ordinary search; `None` means cancellation.
fn by_identifier<'r, 'i, T: SearchDocument + 'r>(
    docs: impl Iterator<Item = (&'r T, &'i [Identifier])>,
    query: &str,
    limit: usize,
    scope: Option<&[T::Source]>,
    stop: &dyn Fn() -> bool,
) -> Option<Option<Vec<Hit<'r, T>>>> {
    if stop() {
        return None;
    }
    let Some(wanted) = identifier::parse_query(query) else {
        return Some(None);
    };
    let field = match wanted {
        Identifier::Doi(_) => MatchField::Doi,
        _ => MatchField::Identifier,
    };
    let mut hits = Vec::new();
    for (n, (reference, ids)) in docs.enumerate() {
        if n % STOP_CHECK_INTERVAL == 0 && stop() {
            return None;
        }
        if in_scope(reference, scope) && ids.contains(&wanted) {
            hits.push(Hit {
                reference,
                score: 3000,
                matched: vec![field],
                typo: false,
            });
        }
    }
    if hits.is_empty() {
        return Some(None);
    }
    hits.sort_by(|a, b| {
        b.reference
            .year()
            .cmp(&a.reference.year())
            .then_with(|| a.reference.identity().cmp(b.reference.identity()))
    });
    hits.truncate(limit);
    Some(Some(hits))
}

const STOP_CHECK_INTERVAL: usize = 4096;
const TERM_CHECK_INTERVAL: usize = 64;

fn in_scope<T: SearchDocument>(reference: &T, scope: Option<&[T::Source]>) -> bool {
    scope.is_none_or(|sources| sources.contains(reference.source()))
}

/// When one typo is forgiven for a term.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Typo {
    Never,
    /// The term must be one edit away from a whole word.
    Word,
    /// The last term, still being typed: one edit away from a word's start.
    WordStart,
}

struct Term {
    text: String,
    chars: Vec<char>,
    finder: Finder<'static>,
    /// How often the term was typed; each occurrence counts.
    count: u32,
    /// [`MatchField::bit`]s of the fields the term may match.
    fields: u8,
    /// Short Latin-script terms match only at the start of a word, so `j`
    /// in `j smith` is an initial, not any word containing a j.
    starts_only: bool,
    /// A `#tag` names a whole tag word once it is typed out.
    whole_only: bool,
    typo: Typo,
}

/// A `y:` hint: an exact year or range, or the digits typed so far.
#[derive(Debug, PartialEq)]
enum Years {
    Range(i32, i32),
    Prefix(String),
}

impl Years {
    fn parse(text: &str) -> Option<Years> {
        let digits = |text: &str| !text.is_empty() && text.len() <= 4 && text.bytes().all(|b| b.is_ascii_digit());
        let bound = |text: &str, open: i32| match text.len() {
            0 => Some(open),
            4 if digits(text) => text.parse().ok(),
            // A year still being typed leaves this end open.
            1..=3 if digits(text) => Some(open),
            _ => None,
        };
        match text.split_once(['-', '–', '—']) {
            None if text.len() == 4 && digits(text) => text.parse().ok().map(|year| Years::Range(year, year)),
            None if digits(text) => Some(Years::Prefix(text.into())),
            None => None,
            Some((from, to)) => {
                let (from, to) = (bound(from, i32::MIN)?, bound(to, i32::MAX)?);
                Some(Years::Range(from.min(to), from.max(to)))
            }
        }
    }

    fn contains(&self, year: Option<i32>) -> bool {
        match (self, year) {
            (Years::Range(from, to), Some(year)) => (*from..=*to).contains(&year),
            (Years::Prefix(prefix), Some(year)) => year.to_string().starts_with(prefix.as_str()),
            (_, None) => false,
        }
    }
}

struct Query {
    terms: Vec<Term>,
    years: Option<Years>,
    /// The whole query, normalised, when it has no hints: compared with
    /// citation keys and titles for the exact-match bonus.
    plain: String,
    /// The trimmed query, lowercased character by character with final sigma
    /// folded, so keys are compared without allocating per record.
    exact: String,
}

impl Query {
    /// `None` means cancellation; a query with nothing to search for has no terms.
    fn parse(query: &str, stop: &dyn Fn() -> bool) -> Option<Query> {
        // Each word with the fields it may match, whether it was quoted and
        // whether it is still being typed (the query's last word, no space after).
        let mut words: Vec<(String, u8, bool, bool)> = Vec::new();
        let mut years = None;
        let mut hinted = false;
        let tokens = tokens(query);
        let typing_token = tokens.len().wrapping_sub(1);
        let still_typing = !query.ends_with(char::is_whitespace);
        for (n, token) in tokens.into_iter().enumerate() {
            if n % TERM_CHECK_INTERVAL == 0 && stop() {
                return None;
            }
            let (fields, text) = match hint(token) {
                Some((None, text)) => {
                    hinted = true;
                    years = Years::parse(text.trim_end_matches([',', ';', '.'])).or(years);
                    continue;
                }
                Some((Some(field), text)) => {
                    hinted = true;
                    (field.bit(), text)
                }
                None => (ANY_FIELD, token),
            };
            let typing = n == typing_token && still_typing;
            if text.contains(is_quote) {
                // A quoted phrase is one term, searched as written.
                let phrase = normalize(&text.replace(is_quote, " "));
                if !phrase.is_empty() {
                    words.push((phrase, fields, true, typing));
                }
            } else {
                let text = normalize(text);
                let last = text.split(' ').count() - 1;
                words.extend(
                    text.split(' ')
                        .enumerate()
                        .filter(|(_, word)| !word.is_empty())
                        .map(|(w, word)| (word.into(), fields, false, typing && w == last)),
                );
            }
        }
        // "et al." and "and" between names say nothing about a record.
        let kept: Vec<_> = words
            .iter()
            .enumerate()
            .filter(|(n, (word, fields, quoted, _))| {
                let next = words.get(n + 1).map(|(word, ..)| word.as_str());
                let previous = n.checked_sub(1).map(|n| words[n].0.as_str());
                let plain = |n: usize| {
                    words
                        .get(n)
                        .is_some_and(|(_, fields, quoted, _)| *fields == ANY_FIELD && !quoted)
                };
                let between = *n > 0 && plain(n - 1) && plain(n + 1);
                *fields != ANY_FIELD
                    || *quoted
                    || !(word == "and" && between
                        || word == "et" && next == Some("al")
                        || word == "al" && previous == Some("et"))
            })
            .map(|(_, word)| word.clone())
            .collect();
        if !kept.is_empty() || hinted {
            words = kept;
        }
        let mut terms: Vec<Term> = Vec::new();
        let mut seen = BTreeMap::new();
        for (n, (text, fields, quoted, typing)) in words.into_iter().enumerate() {
            if n % TERM_CHECK_INTERVAL == 0 && stop() {
                return None;
            }
            let chars: Vec<char> = text.chars().collect();
            let tag = fields == MatchField::Keyword.bit();
            let typo = if tag
                || quoted
                || !(5..=MAX_TYPO_TERM).contains(&chars.len())
                || !chars.iter().all(|c| c.is_alphabetic() && (*c as u32) < 0x0530)
            {
                Typo::Never
            } else if typing {
                Typo::WordStart
            } else {
                Typo::Word
            };
            let whole_only = tag && !typing;
            match seen.entry((text.clone(), fields, typo as u8, whole_only)) {
                std::collections::btree_map::Entry::Occupied(same) => {
                    let same: &mut Term = &mut terms[*same.get()];
                    same.count = same.count.wrapping_add(1);
                    continue;
                }
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(terms.len());
                }
            }
            terms.push(Term {
                finder: Finder::new(text.as_bytes()).into_owned(),
                // Tags are matched from their start, like short words.
                starts_only: tag || chars.len() < 3 && !chars.iter().copied().any(is_cjk_char),
                whole_only,
                chars,
                text,
                count: 1,
                fields,
                typo,
            });
        }
        // Longer terms are rarer, so records fail sooner on them.
        terms.sort_by_key(|term| std::cmp::Reverse(term.text.len()));
        Some(Query {
            terms,
            years,
            plain: if hinted { String::new() } else { normalize(query) },
            exact: query.trim().chars().flat_map(lower).collect(),
        })
    }
}

fn is_quote(c: char) -> bool {
    matches!(c, '"' | '“' | '”')
}

/// Whitespace-separated tokens, except that quotes keep a phrase, and any
/// hint written before it, together: `t:"cell signaling" 2020`.
fn tokens(query: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = None;
    let mut quoted = false;
    for (at, c) in query.char_indices() {
        if is_quote(c) {
            quoted = !quoted;
        } else if c.is_whitespace() && !quoted {
            if let Some(start) = start.take() {
                tokens.push(&query[start..at]);
            }
            continue;
        }
        start.get_or_insert(at);
    }
    tokens.extend(start.map(|start| &query[start..]));
    tokens
}

/// A field hint at the start of `token`: `Some((None, …))` for years.
fn hint(token: &str) -> Option<(Option<MatchField>, &str)> {
    if let Some(tag) = token.strip_prefix('#').filter(|tag| !tag.is_empty()) {
        return Some((Some(MatchField::Keyword), tag));
    }
    let (name, text) = token.split_once(':')?;
    let field = match name.to_ascii_lowercase().as_str() {
        "a" | "author" => MatchField::Author,
        "t" | "title" => MatchField::Title,
        "j" | "journal" => MatchField::Container,
        "k" | "key" => MatchField::Citekey,
        "y" | "year" => return Some((None, text)),
        _ => return None,
    };
    Some((Some(field), text))
}

struct Candidate<'r, T> {
    /// 3 for an exact citation key, 2 for a query equal to the key field,
    /// 1 for an exact title, otherwise 0.
    exact: u8,
    pass: Pass,
    score: u32,
    matched: u8,
    reference: &'r T,
}

/// The passes that look for matches, cheapest and most certain first. A later
/// pass runs only when the earlier ones found fewer than `limit` records, and
/// what it adds ranks below everything they found, so a smaller limit always
/// gives the start of a larger one (as Typesense tries typos only when needed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Pass {
    /// Every term as typed, in keys, titles, names, years, identifiers, tags
    /// and journals; also every exact citation key.
    Exact,
    /// The same fields, with one typo in one term.
    Typo,
    /// Abstracts too, every term as typed.
    Abstract,
}

/// A record's match so far: its number, score and matched field bits.
type Partial = (usize, u32, u8);

/// Rank `docs`, whose text is `texts`, with the passes of [`Pass`].
fn rank<'r, T: SearchDocument>(
    docs: &'r [T],
    texts: &Texts,
    query: &str,
    limit: usize,
    scope: Option<&[T::Source]>,
    stop: &dyn Fn() -> bool,
) -> Option<Vec<Hit<'r, T>>> {
    let query = Query::parse(query, stop)?;
    if limit == 0 {
        return Some(Vec::new());
    }
    let mut found = vec![false; docs.len()];
    let mut candidates: Vec<Candidate<'r, T>> = Vec::new();
    let mut packed = Vec::new();
    let all: Vec<usize> = (0..query.terms.len()).collect();
    // A query of only unfinished or invalid hints searches nothing but keys.
    let searches = !query.terms.is_empty() || query.years.is_some();

    let exact = if searches {
        matching(&query, docs, texts, &all, false, scope, &found, &mut packed, stop)?
    } else {
        Vec::new()
    };
    for (n, score, matched) in exact {
        found[n] = true;
        candidates.push(candidate(&query, &docs[n], texts, n, Pass::Exact, score, matched));
    }
    // An exact key is always found, even one a hint would misread.
    for (n, reference) in docs.iter().enumerate() {
        if n % STOP_CHECK_INTERVAL == 0 && stop() {
            return None;
        }
        if !found[n] && in_scope(reference, scope) && reference.exact_key(&query.exact) {
            found[n] = true;
            candidates.push(Candidate {
                exact: 3,
                pass: Pass::Exact,
                score: 2000,
                matched: MatchField::Citekey.bit(),
                reference,
            });
        }
    }

    // One typo in one term, tried only in records that match every other term.
    // Every term is tried once this pass starts, so its results rank alike
    // whatever the limit.
    // Long pasted queries are not typed by hand and get no typo allowance,
    // which also bounds this pass to a few scans.
    let mut word = Vec::new();
    let typos = candidates.len() < limit && query.terms.len() <= MAX_TYPO_QUERY_TERMS;
    for (t, term) in query.terms.iter().enumerate() {
        if !typos || term.typo == Typo::Never {
            continue;
        }
        let others: Vec<usize> = all.iter().copied().filter(|&other| other != t).collect();
        let near = matching(&query, docs, texts, &others, false, scope, &found, &mut packed, stop)?;
        for (i, (n, score, matched)) in near.into_iter().enumerate() {
            if i % STOP_CHECK_INTERVAL == 0 && stop() {
                return None;
            }
            if let Some((bit, weight)) = best_typo(term, texts, n, &mut word, stop)? {
                found[n] = true;
                let score = weighted_score(score, weight, term.count);
                candidates.push(candidate(
                    &query,
                    &docs[n],
                    texts,
                    n,
                    Pass::Typo,
                    score,
                    matched | 1 << bit,
                ));
            }
        }
    }

    if candidates.len() < limit && !query.terms.is_empty() {
        for (n, score, matched) in matching(&query, docs, texts, &all, true, scope, &found, &mut packed, stop)? {
            candidates.push(candidate(&query, &docs[n], texts, n, Pass::Abstract, score, matched));
        }
    }

    // Ordered rules, each breaking the ties of the one before, as Meilisearch
    // and Algolia rank: an exact key or title, then the pass that found the
    // record (as typed, one typo, abstract), then the field-weighted score
    // (better fields and whole words first), then newest year, then a stable order.
    let order = |a: &Candidate<T>, b: &Candidate<T>| {
        b.exact
            .cmp(&a.exact)
            .then(a.pass.cmp(&b.pass))
            .then(b.score.cmp(&a.score))
            .then_with(|| b.reference.year().cmp(&a.reference.year()))
            .then_with(|| a.reference.identity().cmp(b.reference.identity()))
    };
    if candidates.len() > limit {
        candidates.select_nth_unstable_by(limit - 1, order);
        candidates.truncate(limit);
    }
    candidates.sort_unstable_by(order);
    Some(
        candidates
            .into_iter()
            .map(|candidate| Hit {
                reference: candidate.reference,
                score: candidate.score,
                matched: MatchField::ALL
                    .iter()
                    .filter(|field| candidate.matched & field.bit() != 0)
                    .copied()
                    .collect(),
                typo: candidate.pass == Pass::Typo,
            })
            .collect(),
    )
}

fn candidate<'r, T: SearchDocument>(
    query: &Query,
    reference: &'r T,
    texts: &Texts,
    n: usize,
    pass: Pass,
    score: u32,
    matched: u8,
) -> Candidate<'r, T> {
    let plain = !query.plain.is_empty();
    let exact = if pass == Pass::Exact && reference.exact_key(&query.exact) {
        3
    } else if plain && texts.field(n, 0) == query.plain {
        2
    } else {
        u8::from(plain && texts.title_proper(n) == query.plain)
    };
    Candidate {
        exact,
        pass,
        score: weighted_score(score, [0, 500, 1000, 2000][usize::from(exact)], 1),
        matched: matched | if exact == 3 { MatchField::Citekey.bit() } else { 0 },
        reference,
    }
}

/// The records in scope, within the query's years and not yet `found`, that
/// match every term in `terms` as typed, in record order. The first term is
/// found in every record with one scan; the rest are looked for only in the
/// records left, unless most records are still left. `None` means cancellation.
#[allow(clippy::too_many_arguments)]
fn matching<T: SearchDocument>(
    query: &Query,
    docs: &[T],
    texts: &Texts,
    terms: &[usize],
    abstracts: bool,
    scope: Option<&[T::Source]>,
    found: &[bool],
    packed: &mut Vec<u16>,
    stop: &dyn Fn() -> bool,
) -> Option<Vec<Partial>> {
    let (year_score, year_bit) = match &query.years {
        Some(_) => (MatchField::Year.weight() * WHOLE_WORD, MatchField::Year.bit()),
        None => (0, 0),
    };
    let usable = |n: usize| {
        !found[n]
            && in_scope(&docs[n], scope)
            && query.years.as_ref().is_none_or(|years| years.contains(docs[n].year()))
    };
    let mut records: Vec<Partial> = Vec::new();
    let Some((&first, rest)) = terms.split_first() else {
        for n in 0..docs.len() {
            if n % STOP_CHECK_INTERVAL == 0 && stop() {
                return None;
            }
            if usable(n) {
                records.push((n, year_score, year_bit));
            }
        }
        return Some(records);
    };
    scan(&query.terms[first], texts, abstracts, packed, stop)?;
    for (n, &best) in packed.iter().enumerate() {
        if best != 0 && usable(n) {
            let (weight, bit) = unpack(best);
            records.push((
                n,
                weighted_score(year_score, weight, query.terms[first].count),
                year_bit | 1 << bit,
            ));
        }
    }
    for &t in rest {
        if records.is_empty() {
            break;
        }
        if stop() {
            return None;
        }
        let term = &query.terms[t];
        let global = records.len() * 8 > docs.len();
        if global {
            scan(term, texts, abstracts, packed, stop)?;
        }
        let mut kept = 0;
        for i in 0..records.len() {
            if i % STOP_CHECK_INTERVAL == 0 && stop() {
                return None;
            }
            let (n, score, matched) = records[i];
            let best = if global {
                packed[n]
            } else {
                best_in(term, texts, n, abstracts, stop)?
            };
            if best != 0 {
                let (weight, bit) = unpack(best);
                records[kept] = (n, weighted_score(score, weight, term.count), matched | 1 << bit);
                kept += 1;
            }
        }
        records.truncate(kept);
    }
    Some(records)
}

/// A match's weight and field bit in one number that orders like the pair:
/// the higher weight wins and, on a tie, the later field. Zero is no match.
fn pack(weight: u32, bit: usize) -> u16 {
    (weight << 3 | bit as u32) as u16
}
fn unpack(packed: u16) -> (u32, usize) {
    (u32::from(packed >> 3), usize::from(packed & 7))
}

/// Set `packed[n]` to the best match of `term` in record `n`, for every
/// record, scanning each buffer once. `None` means cancellation.
fn scan(term: &Term, texts: &Texts, abstracts: bool, packed: &mut Vec<u16>, stop: &dyn Fn() -> bool) -> Option<()> {
    packed.clear();
    packed.resize(texts.records.len(), 0);
    let buffers: &[(bool, &str)] = if abstracts {
        &[(false, &texts.meta), (true, &texts.abstracts)]
    } else {
        &[(false, &texts.meta)]
    };
    for &(is_abstract, buffer) in buffers {
        let bytes = buffer.as_bytes();
        let mut n = 0;
        for (count, at) in occurrences(&term.finder, bytes).enumerate() {
            if (count + 1) % STOP_CHECK_INTERVAL == 0 && stop() {
                return None;
            }
            let end = |n: usize| {
                let bounds = &texts.records[n];
                if is_abstract {
                    bounds.abstract_end
                } else {
                    bounds.ends[6]
                }
            };
            while end(n) as usize <= at {
                n += 1;
            }
            let rated = rate(term, texts, n, is_abstract, bytes, at);
            if rated > packed[n] {
                packed[n] = rated;
            }
        }
    }
    Some(())
}

/// Every place `finder` occurs in `bytes`, overlapping ones included, so an
/// occurrence that does not count cannot hide one that does.
fn occurrences<'a>(finder: &'a Finder<'static>, bytes: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    let mut from = 0;
    std::iter::from_fn(move || {
        let at = from + finder.find(bytes.get(from..)?)?;
        from = at + 1;
        Some(at)
    })
}

/// The best match of `term` in record `n` alone, packed. `None` means cancellation.
fn best_in(term: &Term, texts: &Texts, n: usize, abstracts: bool, stop: &dyn Fn() -> bool) -> Option<u16> {
    let (meta, abstract_start) = texts.starts(n);
    let bounds = &texts.records[n];
    let mut best = 0;
    let buffers = [
        (false, texts.meta.as_bytes(), meta, bounds.ends[6] as usize),
        (
            true,
            texts.abstracts.as_bytes(),
            abstract_start,
            bounds.abstract_end as usize,
        ),
    ];
    for (is_abstract, bytes, start, end) in buffers.into_iter().take(1 + usize::from(abstracts)) {
        for (count, at) in occurrences(&term.finder, &bytes[start..end]).enumerate() {
            if (count + 1) % STOP_CHECK_INTERVAL == 0 && stop() {
                return None;
            }
            best = best.max(rate(term, texts, n, is_abstract, bytes, start + at));
        }
    }
    Some(best)
}

/// How the occurrence of `term` at `at` of record `n` counts, packed: a whole
/// word, the start of a word or inside one, times the weight of the field it
/// falls in. Zero when it does not count.
fn rate(term: &Term, texts: &Texts, n: usize, is_abstract: bool, bytes: &[u8], at: usize) -> u16 {
    let after = at + term.text.len();
    // Every field has a space before it and a space or the buffer's end after.
    let starts = at.checked_sub(1).is_none_or(|before| bytes[before] == b' ');
    let ends = bytes.get(after).is_none_or(|&next| next == b' ');
    if !starts && (term.starts_only || term.whole_only) || !ends && term.whole_only {
        return 0;
    }
    let bit = if is_abstract {
        7
    } else {
        texts.records[n].ends.partition_point(|&end| end as usize <= at)
    };
    let field = MatchField::ALL[bit];
    if term.fields & field.bit() == 0 || after > texts.span(n, bit).1 {
        return 0;
    }
    let tier = match field {
        // A year is matched only as a whole.
        MatchField::Year if starts && ends => WHOLE_WORD,
        MatchField::Year => return 0,
        _ if starts && ends => WHOLE_WORD,
        _ if starts => WORD_START,
        _ => INSIDE_WORD,
    };
    pack(field.weight() * tier, bit)
}

/// The field and weight of the term's best match with one typo in record `n`.
/// `None` means cancellation.
fn best_typo(
    term: &Term,
    texts: &Texts,
    n: usize,
    word: &mut Vec<char>,
    stop: &dyn Fn() -> bool,
) -> Option<Option<(usize, u32)>> {
    let mut best = None;
    for (bit, field) in MatchField::ALL.iter().enumerate() {
        if term.fields & TYPO_FIELDS & field.bit() != 0
            && has_typo_match(texts.field(n, bit), term, word, stop)?
            && best.is_none_or(|(_, previous)| field.weight() >= previous)
        {
            best = Some((bit, field.weight()));
        }
    }
    Some(best.map(|(bit, weight)| (bit, weight * ONE_TYPO)))
}

/// Whether a word in `text` is one typo away from `term`, as [`Typo`] allows.
/// `None` means cancellation.
fn has_typo_match(text: &str, term: &Term, word: &mut Vec<char>, stop: &dyn Fn() -> bool) -> Option<bool> {
    let bytes = text.as_bytes();
    let prefix = term.typo == Typo::WordStart;
    let ascii = term.text.is_ascii();
    // One edit never reaches past two more letters than the term has.
    let reach = term.chars.len() + 2;
    for (count, at) in memchr::memchr_iter(term.text.as_bytes()[0], bytes).enumerate() {
        if (count + 1) % STOP_CHECK_INTERVAL == 0 && stop() {
            return None;
        }
        if at > 0 && bytes[at - 1] != b' ' {
            continue;
        }
        let end = memchr::memchr(b' ', &bytes[at..]).map_or(bytes.len(), |end| at + end);
        let near = if ascii {
            one_edit(term.text.as_bytes(), &bytes[at..end.min(at + reach)], prefix)
        } else {
            word.clear();
            word.extend(text[at..end].chars().take(reach));
            one_edit(&term.chars, word, prefix)
        };
        if near {
            return Some(true);
        }
    }
    Some(false)
}

/// Whether `word`, or with `prefix` some start of it, is one edit away from
/// `term`: one letter changed, added or left out, or two neighbours swapped.
/// The first letter must be right; an exact match is not an edit.
fn one_edit<T: PartialEq>(term: &[T], word: &[T], prefix: bool) -> bool {
    let same = term.iter().zip(word).take_while(|(a, b)| a == b).count();
    if same == 0 {
        return false;
    }
    let (t, w) = (&term[same..], &word[same..]);
    let fits = |t: &[T], w: &[T]| if prefix { w.starts_with(t) } else { w == t };
    if t.is_empty() {
        // The term is a start of the word: one letter short of it.
        return !prefix && w.len() == 1;
    }
    (!w.is_empty() && fits(&t[1..], &w[1..]))
        || (t.len() > 1 && w.len() > 1 && t[0] == w[1] && t[1] == w[0] && fits(&t[2..], &w[2..]))
        || (!w.is_empty() && fits(t, &w[1..]))
        || fits(&t[1..], w)
}

// Preserve the original release arithmetic even for enormous queries. Grouped
// or reordered terms must not introduce an overflow panic in checked builds.
fn weighted_score(score: u32, weight: u32, count: u32) -> u32 {
    score.wrapping_add(weight.wrapping_mul(count))
}

/// Lowercase, strip accents, fold letter variants and turn punctuation into spaces.
///
/// Accents are removed only where they are optional in practice: Latin,
/// Greek and Cyrillic diacritics, Arabic vowel signs and Hebrew points. Marks
/// that make a different letter in other scripts, such as Japanese voicing
/// marks and Indic vowel signs, are kept, so が does not match か.
///
/// Folding covers letters whose lowercase form depends on position or that
/// have common alternative spellings: Greek final sigma (ς → σ), German
/// sharp s (ß → ss), long s (ſ → s), Latin letters that do not decompose
/// (ł, ø, æ, œ, đ, dotless ı), Persian and Arabic forms of kaf and yeh,
/// the Arabic tatweel used for stretching words, and katakana to hiragana.
pub fn normalize(text: &str) -> String {
    // Most text is ASCII, which needs neither decomposition nor folding.
    if text.is_ascii() {
        let mut out = String::with_capacity(text.len());
        for word in text
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| !w.is_empty())
        {
            if !out.is_empty() {
                out.push(' ');
            }
            out.extend(word.chars().map(|c| c.to_ascii_lowercase()));
        }
        return out;
    }
    let folded: String = text
        .nfkd()
        .filter(|c| !is_optional_mark(*c))
        .flat_map(char::to_lowercase)
        .flat_map(|c| match c {
            'ς' => ['σ', '\0'],
            'ß' => ['s', 's'],
            'ſ' => ['s', '\0'],
            'ł' => ['l', '\0'],
            'ø' => ['o', '\0'],
            'æ' => ['a', 'e'],
            'œ' => ['o', 'e'],
            'đ' => ['d', '\0'],
            'ı' => ['i', '\0'],
            '\u{06A9}' => ['\u{0643}', '\0'],
            '\u{06CC}' | '\u{0649}' => ['\u{064A}', '\0'],
            '\u{0640}' => ['\0', '\0'],
            '\u{30A1}'..='\u{30F6}' => [char::from_u32(c as u32 - 0x60).unwrap_or(c), '\0'],
            c if c.is_alphanumeric() || is_combining_mark(c) => [c, '\0'],
            _ => [' ', '\0'],
        })
        .filter(|c| *c != '\0')
        .collect();
    // Recompose kept marks and Hangul syllables; most text has neither, so it
    // skips this second pass.
    let folded: String = if folded
        .chars()
        .any(|c| is_combining_mark(c) || ('\u{1100}'..='\u{11FF}').contains(&c))
    {
        folded.nfc().collect()
    } else {
        folded
    };
    // A kept mark left at the start of a word (after punctuation or a spacing
    // mark such as ゛) has no letter to belong to.
    folded
        .split_whitespace()
        .map(|word| word.trim_start_matches(is_combining_mark))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Combining marks that readers treat as optional, so searches ignore them.
fn is_optional_mark(c: char) -> bool {
    matches!(c,
        '\u{0300}'..='\u{036F}'
        | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}'
        | '\u{20D0}'..='\u{20FF}'
        | '\u{FE20}'..='\u{FE2F}'
        | '\u{0483}'..='\u{0489}'
        | '\u{0591}'..='\u{05BD}'
        | '\u{05BF}'
        | '\u{05C1}'..='\u{05C2}'
        | '\u{05C4}'..='\u{05C5}'
        | '\u{05C7}'
        | '\u{0610}'..='\u{061A}'
        | '\u{064B}'..='\u{065F}'
        | '\u{0670}'
        | '\u{06D6}'..='\u{06ED}'
        // Variation selectors pick a glyph, not a different letter.
        | '\u{180B}'..='\u{180D}'
        | '\u{180F}'
        | '\u{FE00}'..='\u{FE0F}'
        | '\u{E0100}'..='\u{E01EF}')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Issued, Name, RecordId};

    fn reference(key: &str, title: &str, family: &str, year: i32) -> Reference {
        Reference {
            id: RecordId {
                source: "t".into(),
                native_key: key.into(),
            },
            citekey: Some(key.into()),
            item_type: "article-journal".into(),
            title: Some(title.into()),
            authors: vec![Name {
                family: Some(family.into()),
                ..Name::default()
            }],
            editors: vec![],
            other_names: Default::default(),
            dates: Default::default(),
            issued: Some(Issued::year(year)),
            container_title: None,
            doi: None,
            url: None,
            abstract_text: None,
            keywords: vec![],
            language: None,
            eprints: vec![],
            variables: Default::default(),
            attachment: None,
            raw: None,
        }
    }

    #[test]
    fn container_search_ignores_rich_text_markup_and_entities() {
        let mut r = reference("k", "Title", "Smith", 2020);
        r.container_title = Some("<i>Q</i> &amp; A".into());
        let refs = [r];
        let index = OwnedIndex::new(refs.into());
        for query in ["q & a", "Q A", "q&a"] {
            let hits = index.search(query, 10);
            assert_eq!(hits.len(), 1, "{query}");
            assert!(hits[0].matched.contains(&MatchField::Container), "{query}");
        }
        assert!(index.search("amp", 10).is_empty());
        assert_eq!(index.texts.field(0, 6), "q a");
    }

    #[test]
    fn source_scopes_filter_before_limits_for_words_unicode_and_identifiers() {
        let mut refs = crate::bibtex::load_str(
            "zotero:user",
            "@article{same,title={García 研究},doi={10.1000/scope},isbn={0-306-40615-2},pmid={12345},eprint={2101.00001},archiveprefix={arXiv},year={2026}}",
        )
        .references;
        for n in 0..3 {
            let mut r = refs[0].clone();
            r.id.source = "zotero:group:7".into();
            r.id.native_key = format!("group{n}");
            r.issued = Some(Issued::year(2020 + n));
            refs.push(r);
        }
        let owned = OwnedIndex::new(refs.clone());
        assert_eq!(owned.source_counts().get("zotero:user"), Some(&1));
        assert_eq!(owned.source_counts().get("zotero:group:7"), Some(&3));
        assert_eq!(owned.source_counts().values().sum::<usize>(), owned.references().len());
        let group = vec!["zotero:group:7".into()];
        let all = vec!["zotero:user".into(), "zotero:group:7".into()];
        for query in [
            "garcia",
            "研究",
            "10.1000/scope",
            "978-0-306-40615-7",
            "PMID:12345",
            "arXiv:2101.00001",
        ] {
            for scope in [
                None,
                Some(all.as_slice()),
                Some(group.as_slice()),
                Some(&[]),
                Some(&["missing".into()][..]),
            ] {
                let expected: Vec<_> = owned
                    .search(query, 10)
                    .into_iter()
                    .filter(|h| scope.is_none_or(|s| s.contains(&h.reference.id.source)))
                    .take(2)
                    .map(|h| h.reference.id.clone())
                    .collect();
                let hits = owned.search_scoped(query, 2, scope);
                assert_eq!(
                    hits.iter().map(|h| h.reference.id.clone()).collect::<Vec<_>>(),
                    expected
                );
            }
            assert!(owned.search_scoped(query, 0, Some(&group)).is_empty());
            assert!(owned.search_scoped_until(query, 2, Some(&group), &|| true).is_none());
        }
        assert!(owned.search_scoped("", 2, Some(&group)).is_empty());
        // An identifier outside this scope must not suppress an ordinary match inside it.
        let mut fallback = reference("fallback", "10.1000/scope", "Example", 2020);
        fallback.id.source = "file".into();
        refs.push(fallback);
        let owned = OwnedIndex::new(refs);
        let hits = owned.search_scoped("10.1000/scope", 1, Some(&["file".into()]));
        assert_eq!(hits[0].reference.id.native_key, "fallback");
        assert_eq!(hits[0].matched, [MatchField::Title]);
    }

    #[test]
    fn scoped_search_cancels_when_a_concurrent_request_supersedes_it() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        let mut r = reference("a", "Synthetic research", "Example", 2020);
        r.doi = Some("10.1000/scope".into());
        let index = OwnedIndex::new(vec![r; 10_000]);
        for query in ["research", "10.1000/scope"] {
            let cancelled = AtomicBool::new(false);
            let (paused, ready) = mpsc::channel();
            let (resume, wait) = mpsc::channel();
            std::thread::scope(|threads| {
                let index = &index;
                let cancelled = &cancelled;
                let running = threads.spawn(move || {
                    let checks = std::cell::Cell::new(0);
                    let stop = || {
                        checks.set(checks.get() + 1);
                        if checks.get() == 3 {
                            paused.send(()).unwrap_or_else(|e| panic!("{e}"));
                            wait.recv_timeout(std::time::Duration::from_secs(5))
                                .unwrap_or_else(|e| panic!("{e}"));
                        }
                        cancelled.load(Ordering::SeqCst)
                    };
                    index.search_scoped_until(query, 20, Some(&["excluded".into()]), &stop)
                });
                ready
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap_or_else(|e| panic!("{e}"));
                cancelled.store(true, Ordering::SeqCst);
                resume.send(()).unwrap_or_else(|e| panic!("{e}"));
                assert!(running.join().unwrap_or_else(|_| panic!("search panicked")).is_none());
            });
        }
    }

    /// Manual scale evidence, separated from parsing, IPC and rendering.
    #[test]
    #[ignore = "manual synthetic scale measurement; no timing assertion"]
    fn owned_index_100000_reference_measurement() {
        use std::time::{Duration, Instant};
        let refs: Vec<_> = (0..100_000)
            .map(|i| {
                let mut r = reference(&format!("ref{i:05}"), "Synthetic research 研究", "García", 2024);
                r.id.source = if i < 50_000 { "synthetic-a" } else { "synthetic-b" }.into();
                r.id.native_key = format!("n{:05}", i % 50_000);
                r
            })
            .collect();
        let expected: Vec<_> = refs.iter().take(200).map(|r| r.id.clone()).collect();
        let last = refs[99_999].id.clone();
        let started = Instant::now();
        let index = OwnedIndex::new(refs);
        let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
        eprintln!(
            "synthetic records=100000 profile={profile} index_build_us={}",
            started.elapsed().as_micros()
        );
        assert_eq!(index.references().len(), 100_000);
        for (query, limit, count) in [
            ("r", 20, 20),
            ("研", 20, 20),
            ("research", 20, 20),
            ("research", 200, 200),
            ("研究", 20, 20),
            ("研究", 200, 200),
            ("ref99999", 20, 1),
            ("missing-synthetic-term", 20, 0),
        ] {
            let mut elapsed = Duration::ZERO;
            for _ in 0..10 {
                let started = Instant::now();
                let hits = index.search(query, limit);
                elapsed += started.elapsed();
                assert_eq!(hits.len(), count);
                let ids: Vec<_> = hits.iter().map(|hit| hit.reference.id.clone()).collect();
                if count == 1 {
                    assert_eq!(ids.as_slice(), std::slice::from_ref(&last));
                } else {
                    assert_eq!(ids, expected[..count]);
                }
            }
            eprintln!(
                "query={query:?} limit={limit} returned={count} samples=10 mean_us={}",
                elapsed.as_micros() / 10
            );
        }
    }

    #[test]
    fn accents_and_case_are_ignored() {
        let refs = [reference("garcia2022", "Sueño y memoria", "García", 2022)];
        let index = OwnedIndex::new(refs.into());
        let hits = index.search("GARCIA sueno", 10);
        assert_eq!(hits.len(), 1);
        // A whole surname is better evidence than the start of a key.
        assert_eq!(hits[0].matched, [MatchField::Title, MatchField::Author]);
    }

    #[test]
    fn all_terms_must_match() {
        let refs = [
            reference("a", "Sleep and memory", "Smith", 2020),
            reference("b", "Sleep and diet", "Jones", 2021),
        ];
        let index = OwnedIndex::new(refs.into());
        let hits = index.search("sleep memory", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].reference.id.native_key, "a");
    }

    #[test]
    fn long_queries_can_cancel_before_finishing_the_first_record() {
        use std::cell::Cell;
        let many_terms = (0..512).map(|n| format!("term{n}")).collect::<Vec<_>>().join(" ");
        for (query, title) in [
            (many_terms.as_str(), many_terms.clone()),
            ("a", format!("b{}", "a".repeat(65_535))),
        ] {
            let r = reference("k", &title, "Example", 2024);
            let mut texts = Texts::default();
            prepare(&r, &mut texts);
            let parse_checks = Cell::new(0);
            assert!(
                Query::parse(query, &|| {
                    parse_checks.set(parse_checks.get() + 1);
                    false
                })
                .is_some()
            );
            // Cancellation becomes visible only after query preparation, while
            // processing one matching record, without relying on elapsed time.
            let checks = Cell::new(0);
            assert!(
                rank(std::slice::from_ref(&r), &texts, query, 10, None, &|| {
                    checks.set(checks.get() + 1);
                    checks.get() > parse_checks.get() + 1
                })
                .is_none(),
                "{}",
                &query[..10.min(query.len())]
            );
        }
    }

    #[test]
    fn repeated_folded_terms_keep_scores_explanations_and_order() {
        let mut red = reference("r", "Red", "Example", 2000);
        red.abstract_text = Some("Blue".into());
        let mut blue = reference("b", "Blue", "Example", 2025);
        blue.abstract_text = Some("Red".into());
        let refs = [blue, red];
        let owned = OwnedIndex::new(refs.into());
        let hits = owned.search("RÉD red blue", 10);
        assert_eq!(
            hits.iter()
                .map(|h| (&*h.reference.id.native_key, h.score, h.matched.clone()))
                .collect::<Vec<_>>(),
            [
                ("r", 84, vec![MatchField::Title, MatchField::Abstract]),
                ("b", 48, vec![MatchField::Title, MatchField::Abstract]),
            ]
        );
        let hits = owned.search_scoped("red red blue", 1, Some(&["t".into()]));
        assert_eq!(hits[0].reference.id.native_key, "r");
        assert!(owned.search_scoped("red red blue", 1, Some(&[])).is_empty());
        // Coalescing terms must not turn a repeated query into an exact match.
        assert_eq!(owned.search("red red", 1)[0].score, 80);
        let exact = OwnedIndex::new(vec![reference("red red", "red red", "Example", 2024)]);
        assert_eq!(exact.search("red red", 1)[0].score, 2096);
    }

    #[test]
    fn huge_multiplicities_preserve_release_scores_without_allocating_the_query() {
        // This count would require a 341 MiB query. It must not panic before
        // a later missing term can reject the record.
        assert_eq!(weighted_score(0, 24, 178_956_971), 8);
        assert_eq!(weighted_score(u32::MAX - 1, 2, 1), 0);
        assert_eq!(weighted_score(8, 2000, 1), 2008);
    }

    #[test]
    #[ignore = "manual synthetic repetitive-query measurement; no timing assertion"]
    fn repetitive_query_measurement() {
        use std::time::{Duration, Instant};
        let index = OwnedIndex::new(
            (0..5_000)
                .map(|n| reference(&format!("key{n}"), "Synthetic research", "Example", 2024))
                .collect(),
        );
        for query in ["research".to_owned(), "research ".repeat(1_000)] {
            let mut elapsed = Duration::ZERO;
            for _ in 0..10 {
                let started = Instant::now();
                let hits = index.search(&query, 20);
                elapsed += started.elapsed();
                assert_eq!(hits.len(), 20);
                assert_eq!(hits[0].score, 20 * query.split_whitespace().count() as u32);
            }
            eprintln!(
                "records=5000 query_bytes={} samples=10 mean_us={}",
                query.len(),
                elapsed.as_micros() / 10
            );
        }
    }

    #[test]
    fn exact_key_ranks_first_and_year_matches_whole_number() {
        let refs = [
            reference("smith2020", "On smith2020 data", "Other", 2019),
            reference("smith2020x", "Unrelated", "Smith", 2020),
        ];
        let index = OwnedIndex::new(refs.into());
        assert_eq!(index.search("smith2020", 10)[0].reference.id.native_key, "smith2020");
        assert!(
            index
                .search("202", 10)
                .iter()
                .all(|h| !h.matched.contains(&MatchField::Year))
        );
    }

    #[test]
    fn greek_final_sigma_and_german_sharp_s_match_their_plain_forms() {
        let refs = [
            reference("gr", "Οδός προς την ΓΝΩΣΙΣ", "Παπαδόπουλος", 2020),
            reference("de", "Die Straße der Erinnerung", "Weiß", 2021),
        ];
        let index = OwnedIndex::new(refs.into());
        assert_eq!(index.search("γνωσις", 10)[0].reference.id.native_key, "gr");
        assert_eq!(index.search("ΓΝΏΣΙΣ", 10)[0].reference.id.native_key, "gr");
        assert_eq!(index.search("strasse weiss", 10)[0].reference.id.native_key, "de");
    }

    #[test]
    fn exact_key_match_is_exact_not_punctuation_folded() {
        let refs = [
            reference("a:b", "Newer", "X", 2024),
            reference("a-b", "Older", "Y", 2001),
        ];
        let index = OwnedIndex::new(refs.into());
        assert_eq!(index.search("a-b", 10)[0].reference.id.native_key, "a-b");
        assert_eq!(index.search("a:b", 10)[0].reference.id.native_key, "a:b");
    }

    #[test]
    fn translators_and_other_roles_are_searchable() {
        let refs = crate::bibtex::load_str(
            "t",
            "@book{translation,title={Synthetic},translator={Translator, Taylor},year={2024}}\n@book{compilation,title={Another},editor={Compiler, Casey},editortype={compiler},year={2024}}",
        )
        .references;
        let index = OwnedIndex::new(refs);
        assert_eq!(index.search("Translator", 10).len(), 1);
        assert_eq!(index.search("Compiler", 10).len(), 1);
    }

    #[test]
    fn cjk_and_arabic_substrings_match() {
        let refs = [
            reference("zh", "睡眠与记忆", "示例", 2025),
            reference("ar", "النوم والذاكرة", "مثال", 2021),
        ];
        let index = OwnedIndex::new(refs.into());
        assert_eq!(index.search("记忆", 10)[0].reference.id.native_key, "zh");
        assert_eq!(index.search("الذاكرة", 10)[0].reference.id.native_key, "ar");
    }

    #[test]
    fn latin_letters_without_decomposition_match_their_plain_forms() {
        let refs = [
            reference("pl", "Historia Łodzi", "Kowalczyk", 2020),
            reference("no", "Søvn og hukommelse", "Ødegård", 2021),
            reference("fr", "Œuvres et Cæsar", "Đorđević", 2022),
            reference("tr", "Uykunun ışığı", "Yılmaz", 2023),
        ];
        let index = OwnedIndex::new(refs.into());
        assert_eq!(index.search("lodzi", 10)[0].reference.id.native_key, "pl");
        assert_eq!(index.search("sovn odegard", 10)[0].reference.id.native_key, "no");
        assert_eq!(
            index.search("oeuvres caesar dordevic", 10)[0].reference.id.native_key,
            "fr"
        );
        assert_eq!(index.search("isigi yilmaz", 10)[0].reference.id.native_key, "tr");
    }

    #[test]
    fn persian_and_arabic_letter_variants_and_tatweel_match() {
        // Arabic kaf and yeh as stored; Persian keyboards type ک and ی.
        let refs = [
            reference("fa", "كتاب الذكي", "علي", 2020),
            reference("tw", "العـــلم", "مثال", 2021),
        ];
        let index = OwnedIndex::new(refs.into());
        assert_eq!(index.search("کتاب", 10)[0].reference.id.native_key, "fa");
        assert_eq!(index.search("علی", 10)[0].reference.id.native_key, "fa");
        assert_eq!(index.search("العلم", 10)[0].reference.id.native_key, "tw");
    }

    #[test]
    fn hiragana_and_katakana_match_each_other() {
        let refs = [reference("ja", "ヤマダの研究", "ヤマダ", 2020)];
        let index = OwnedIndex::new(refs.into());
        assert_eq!(index.search("やまだ", 10).len(), 1);
        assert_eq!(index.search("ﾔﾏﾀﾞ", 10).len(), 1);
    }

    #[test]
    fn marks_that_change_letters_in_other_scripts_are_kept() {
        let refs = [
            reference("ga", "がっこう", "例", 2020),
            reference("ka", "かっこう", "例", 2021),
            reference("hi", "किताब", "उदाहरण", 2022),
        ];
        let index = OwnedIndex::new(refs.into());
        let keys = |q| {
            index
                .search(q, 10)
                .iter()
                .map(|h| h.reference.id.native_key.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(keys("がっこう"), ["ga"]);
        assert_eq!(keys("ガッコウ"), ["ga"]);
        assert_eq!(keys("かっこう"), ["ka"]);
        assert_eq!(keys("किताब"), ["hi"]);
        assert!(keys("कताब").is_empty());

        let refs = [
            reference("vs", "葛\u{E0100}城の研究", "例", 2020),
            reference("he", "בית־ספר", "דוגמה", 2021),
        ];
        let index = OwnedIndex::new(refs.into());
        assert_eq!(index.search("葛城", 10)[0].reference.id.native_key, "vs");
        assert_eq!(index.search("ספר", 10)[0].reference.id.native_key, "he");
        assert_eq!(normalize("❤\u{FE0F}x"), "x");
        assert_eq!(normalize("か゛ き"), "か き");
        assert_eq!(normalize("בית־ספר"), "בית ספר");

        let refs = [reference("ko", "한국어 연구", "김", 2020)];
        let index = OwnedIndex::new(refs.into());
        assert_eq!(index.search("한국", 10).len(), 1);
        assert!(index.search("하국", 10).is_empty());
    }

    #[test]
    fn a_stopped_search_returns_nothing_and_limits_keep_the_best() {
        let refs: Vec<_> = (0..10_000)
            .map(|n| reference(&format!("k{n}"), "Sleep study", "Smith", 2000 + (n % 20)))
            .collect();
        let index = OwnedIndex::new(refs);
        assert!(index.search_until("sleep", 10, &|| true).is_none());
        assert!(index.search("sleep", 0).is_empty());
        let top = index.search("sleep", 5);
        let all = index.search("sleep", 10_000);
        assert_eq!(all.len(), 10_000);
        let ids = |hits: &[Hit]| hits.iter().map(|h| h.reference.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&top), ids(&all[..5]));
        assert_eq!(top[0].matched, [MatchField::Title]);
    }

    #[test]
    fn exact_keys_with_greek_sigma_match_in_any_case() {
        let refs = [
            reference("ΜΑΣ", "Other", "X", 2020),
            reference("μας13", "Other", "Y", 2021),
        ];
        let index = OwnedIndex::new(refs.into());
        for (query, key) in [("μας", "ΜΑΣ"), ("Μας", "ΜΑΣ"), ("ΜΑΣ13", "μας13")] {
            let hits = index.search(query, 10);
            assert_eq!(hits[0].reference.id.native_key, key, "{query}");
            assert!(hits[0].score >= 2000, "{query}");
        }
    }

    #[test]
    fn word_start_matches_score_higher_than_inner_matches() {
        let refs = [
            reference("a", "Insomnia", "X", 2020),
            reference("b", "The somnia", "Y", 2020),
        ];
        let index = OwnedIndex::new(refs.into());
        let hits = index.search("somnia", 10);
        assert_eq!(hits[0].reference.id.native_key, "b");
        assert_eq!(hits[0].score, 2 * hits[1].score);
    }

    #[test]
    fn a_pasted_identifier_finds_the_record_that_carries_it() {
        let refs = crate::bibtex::load_str(
            "t",
            "@article{doi1, title={Sleep}, doi={10.1000/ABC.1}, year={2020}}
             @book{isbn1, title={Memory}, isbn={0-306-40615-2}, year={2019}}
             @article{arx, title={Dreams}, url={https://arxiv.org/abs/2101.00001v2}, year={2021}}
             @article{pm, title={Naps}, url={https://pubmed.ncbi.nlm.nih.gov/12345/}, year={2022}}
             @article{old, title={Strings}, eprint={hep-th/9711200}, archiveprefix={arXiv}, year={1997}}
             @article{other, title={Mentions 10.1000/abc.1 in passing}, year={2023}}",
        )
        .references;
        let owned = OwnedIndex::new(refs);
        for (query, key, field) in [
            ("https://doi.org/10.1000/abc.1", "doi1", MatchField::Doi),
            ("978-0-306-40615-7", "isbn1", MatchField::Identifier),
            ("arXiv:2101.00001", "arx", MatchField::Identifier),
            ("PMID:12345", "pm", MatchField::Identifier),
            ("hep-th/9711200", "old", MatchField::Identifier),
            ("arXiv:hep-th/9711200", "old", MatchField::Identifier),
            ("https://arxiv.org/abs/hep-th/9711200", "old", MatchField::Identifier),
        ] {
            let hits = owned.search(query, 10);
            assert_eq!(hits.len(), 1, "{query}");
            assert_eq!(hits[0].reference.id.native_key, key, "{query}");
            assert_eq!(hits[0].matched, [field], "{query}");
        }
        // An identifier nobody carries falls back to ordinary search.
        assert_eq!(owned.search("10.1000/zzz", 10).len(), 0);
        assert_eq!(owned.search("passing", 10)[0].reference.id.native_key, "other");
    }

    /// A small synthetic library for the forgiving-search rules below.
    fn library() -> OwnedIndex {
        let mut refs = vec![
            reference("smith2020", "Sleep and memory consolidation", "Smith", 2020),
            reference("smyth2019", "Dreams in the laboratory", "Smyth", 2019),
            reference("major2018", "A major review", "Ali", 2018),
            reference("li2021", "Neuroscience of attention", "Li", 2021),
            reference("lily2017", "Gardens", "Lily", 2017),
            reference("trial2016", "A randomised trial", "Jones", 2016),
            reference("trail2015", "The mountain trail", "Brown", 2015),
            reference("orwell1949", "1984", "Orwell", 1949),
            reference("tolkien1954", "The fellowship", "Tolkien", 1954),
            reference("yamada2022", "研究", "山田 太郎", 2022),
            reference("plain2014", "Unrelated title", "Plain", 2014),
        ];
        refs[0].authors.push(Name {
            family: Some("Jones".into()),
            given: Some("Ann".into()),
            ..Name::default()
        });
        refs[0].authors[0].given = Some("John".into());
        refs[0].keywords = vec!["sleep".into(), "memory".into()];
        refs[0].container_title = Some("Journal of Sleep Research".into());
        refs[0]
            .variables
            .insert("container-title-short".into(), "J Sleep Res".into());
        refs[1].keywords = vec!["sleepiness".into()];
        refs[8].authors[0].given = Some("J. R. R.".into());
        refs[9].authors[0] = Name {
            literal: Some("山田 太郎".into()),
            ..Name::default()
        };
        refs[10].abstract_text = Some("A study of sleep and memory in rats".into());
        OwnedIndex::new(refs)
    }

    fn keys(index: &OwnedIndex, query: &str, limit: usize) -> Vec<String> {
        index
            .search(query, limit)
            .iter()
            .map(|hit| hit.reference.id.native_key.clone())
            .collect()
    }

    #[test]
    fn one_typo_is_forgiven_after_every_exact_match() {
        let index = library();
        // Completed words: one letter swapped, changed, missing or extra.
        for query in ["smtih ", "smiht ", "smitj ", "smithh ", "consolidaton "] {
            assert_eq!(keys(&index, query, 10), ["smith2020"], "{query:?}");
            assert!(index.search(query, 10)[0].typo, "{query:?}");
        }
        // A word still being typed may be a misspelt start of a word.
        assert_eq!(keys(&index, "nueros", 10), ["li2021"]);
        assert!(keys(&index, "nueros ", 10).is_empty());
        // Exact matches come first; a close match only follows them.
        assert_eq!(keys(&index, "trial ", 10), ["trial2016", "trail2015"]);
        assert_eq!(keys(&index, "trial ", 1), ["trial2016"]);
        assert!(!index.search("trial ", 1)[0].typo);
        assert_eq!(keys(&index, "smith ", 10), ["smith2020", "smyth2019"]);
    }

    #[test]
    fn typos_are_not_forgiven_where_they_would_mislead() {
        let index = library();
        // The first letter must be right, and only one term may be misspelt.
        assert!(keys(&index, "msith ", 10).is_empty());
        assert!(keys(&index, "smtih slepe ", 10).is_empty());
        // Short words, years and numbers are never guessed.
        assert!(keys(&index, "smih ", 10).is_empty());
        assert!(keys(&index, "lyli ", 10).is_empty());
        assert_eq!(keys(&index, "2021 ", 10), ["li2021"]);
        assert!(keys(&index, "1985 ", 10).is_empty());
    }

    #[test]
    fn short_terms_match_word_starts_and_initials() {
        let index = library();
        // "j" is an initial, not any word containing a j; "li" is a name or
        // the start of one, not the inside of "Ali".
        assert_eq!(keys(&index, "j smith", 10), ["smith2020"]);
        assert_eq!(keys(&index, "li", 10), ["li2021", "lily2017"]);
        assert!(!keys(&index, "ali", 10).contains(&"li2021".to_string()));
        assert_eq!(keys(&index, "jrr tolkien", 10), ["tolkien1954"]);
        assert_eq!(keys(&index, "j r r tolkien", 10), ["tolkien1954"]);
        // Chinese, Japanese and Korean have no spaces between words.
        assert_eq!(keys(&index, "山田太郎", 10), ["yamada2022"]);
        assert_eq!(keys(&index, "田", 10), ["yamada2022"]);
    }

    #[test]
    fn citation_like_queries_ignore_et_al_and_and() {
        let index = library();
        assert_eq!(keys(&index, "Smith et al., 2020", 10)[0], "smith2020");
        assert_eq!(keys(&index, "smith and jones", 10), ["smith2020"]);
        // A bare number is searched everywhere, not only as a year.
        assert_eq!(keys(&index, "1984", 10), ["orwell1949"]);
        assert_eq!(keys(&index, "smith 2020", 10), ["smith2020"]);
    }

    #[test]
    fn field_hints_limit_terms_to_one_field() {
        let index = library();
        assert_eq!(keys(&index, "a:jones", 10), ["smith2020", "trial2016"]);
        assert!(keys(&index, "t:jones", 10).is_empty());
        assert_eq!(keys(&index, "t:trial", 10), ["trial2016", "trail2015"]);
        assert_eq!(keys(&index, "j:sleep", 10), ["smith2020"]);
        assert_eq!(keys(&index, "j:res", 10), ["smith2020"], "journal abbreviation");
        assert_eq!(keys(&index, "y:2019-2020", 10), ["smith2020", "smyth2019"]);
        assert_eq!(keys(&index, "y:-1950", 10), ["orwell1949"]);
        assert_eq!(keys(&index, "y:2015-", 10).len(), 8);
        assert_eq!(keys(&index, "y:194", 10), ["orwell1949"], "a year still being typed");
        assert_eq!(
            keys(&index, "smith y:2020,", 10),
            ["smith2020"],
            "before a typed locator"
        );
        assert!(keys(&index, "smith y:2018", 10).is_empty());
        // A tag is a whole word once typed out, and its start while typing.
        assert_eq!(keys(&index, "#sleep ", 10), ["smith2020"]);
        assert_eq!(keys(&index, "#sleep", 10), ["smith2020", "smyth2019"]);
        // An unfinished hint searches nothing yet; an unknown one is text.
        assert!(keys(&index, "a:", 10).is_empty());
        assert_eq!(keys(&index, "Sleep: and memory", 10)[0], "smith2020");
    }

    #[test]
    fn quotes_keep_a_phrase_together() {
        let index = library();
        assert_eq!(keys(&index, "\"memory consolidation\"", 10), ["smith2020"]);
        assert!(keys(&index, "\"consolidation memory\"", 10).is_empty());
        assert_eq!(keys(&index, "t:\"randomised trial\"", 10), ["trial2016"]);
        assert_eq!(keys(&index, "“mountain trail”", 10), ["trail2015"]);
    }

    #[test]
    fn abstracts_are_searched_only_when_other_fields_find_too_few() {
        let index = library();
        // The title match fills a limit of one; the abstract adds a second.
        assert_eq!(keys(&index, "sleep memory", 1), ["smith2020"]);
        assert_eq!(keys(&index, "sleep memory", 10), ["smith2020", "plain2014"]);
        assert_eq!(index.search("sleep memory", 10)[1].matched, [MatchField::Abstract]);
        assert_eq!(keys(&index, "rats", 10), ["plain2014"]);
    }

    #[test]
    fn a_smaller_limit_is_always_the_start_of_a_larger_one() {
        let index = library();
        for query in [
            "sleep",
            "smith ",
            "trial ",
            "sleep memory",
            "s",
            "y:2015-",
            "1984",
            "nueros",
            "j",
        ] {
            let all = keys(&index, query, 100);
            for limit in 1..all.len() {
                assert_eq!(keys(&index, query, limit), all[..limit], "{query:?} {limit}");
            }
        }
    }

    #[test]
    fn one_edit_covers_each_kind_of_typo() {
        let edit = |term: &str, word: &str, prefix| one_edit(term.as_bytes(), word.as_bytes(), prefix);
        assert!(edit("smtih", "smith", false), "swap");
        assert!(edit("smoth", "smith", false), "change");
        assert!(edit("smth", "smith", false), "missing");
        assert!(edit("smiith", "smith", false), "extra");
        assert!(edit("smit", "smith", false), "last letter missing");
        assert!(!edit("smith", "smith", false), "exact is not an edit");
        assert!(!edit("msith", "smith", false), "first letter");
        assert!(!edit("smtiy", "smith", false), "two edits");
        assert!(edit("nueros", "neuroscience", true), "start of a word");
        assert!(!edit("nueros", "neuroscience", false), "not a whole word");
        assert!(!edit("neuro", "neuroscience", true), "exact start is not an edit");
    }

    #[test]
    fn review_findings_on_hints_tags_and_typing_state() {
        let mut tagged = reference("tagged", "Unrelated", "Example", 2020);
        tagged.keywords = vec!["sleepiness".into()];
        let mut abbreviated = reference("rna", "MicroRNA", "Example", 2019);
        abbreviated.variables.insert("title-short".into(), "RNA".into());
        let mut keyword_only = reference("kw", "Other", "Example", 2018);
        keyword_only.keywords = vec!["sleep".into()];
        let index = OwnedIndex::new(vec![
            tagged,
            abbreviated,
            keyword_only,
            reference("y:smith", "Other", "Example", 2017),
            reference("neuro", "Neuroscience", "Example", 2021),
        ]);
        // A word followed by a hint is finished, not still being typed.
        assert!(keys(&index, "#sleep y:2020", 10).is_empty());
        assert!(keys(&index, "nueros y:2021", 10).is_empty());
        assert_eq!(keys(&index, "#sleep", 10), ["kw", "tagged"]);
        // Tags match from their start only, and are never guessed.
        assert!(keys(&index, "#leep", 10).is_empty());
        assert!(keys(&index, "sleeq ", 10).is_empty());
        // A short title inside a longer word is still its own word.
        assert_eq!(keys(&index, "t:rn", 10), ["rna"]);
        // A key that looks like an unfinished hint is still found exactly.
        assert_eq!(keys(&index, "y:smith", 10), ["y:smith"]);
    }

    #[test]
    fn an_occurrence_that_does_not_count_cannot_hide_one_that_does() {
        let mut first = reference("foo", "foo foo", "Example", 2020);
        first.abstract_text = Some("foo foo".into());
        let index = OwnedIndex::new(vec![first, reference("bar", "foo foo stuff", "Example", 2020)]);
        assert_eq!(keys(&index, "\"foo foo\"", 10), ["foo", "bar"]);
        assert_eq!(keys(&index, "\"foo foo\"", 1), ["foo"]);
    }

    #[test]
    fn hostile_queries_do_bounded_work_and_typos_poll_cancellation() {
        use std::cell::Cell;
        let index = OwnedIndex::new(
            (0..10)
                .map(|n| reference(&format!("k{n}"), "Title", "Example", 2020))
                .collect(),
        );
        let words: String = (0..9_000u32)
            .map(|n| {
                let letters: String = (0..5)
                    .map(|i| char::from(b'a' + ((n / 26u32.pow(i)) % 26) as u8))
                    .collect();
                format!("q{letters} ")
            })
            .collect();
        let checks = Cell::new(0);
        let stop = || {
            checks.set(checks.get() + 1);
            false
        };
        assert!(
            index
                .search_until(&words, 20, &stop)
                .is_some_and(|hits| hits.is_empty())
        );
        // Parsing polls every 64 words; nothing rescans the index per word.
        assert!(checks.get() < 1_000, "{}", checks.get());

        let alphas = vec!["α"; 50_000].join(" ");
        let index = OwnedIndex::new(vec![reference("greek", &alphas, "Example", 2020)]);
        checks.set(0);
        assert!(
            index
                .search_until("αβγδε ", 20, &stop)
                .is_some_and(|hits| hits.is_empty())
        );
        assert!(checks.get() > 10, "the typo pass polls while it scans");
    }
}
