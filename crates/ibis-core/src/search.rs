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
//! "and" are ignored. Müller is also found as "mueller" and β as "beta". An
//! exact citation key always matches and ranks first, then an exact title.
//! When nothing matches every term, the records matching most words are given
//! instead, marked as partial.

use std::collections::{BTreeMap, BTreeSet};

use memchr::memmem::Finder;
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::identifier::{self, Identifier};
use crate::model::{Name, is_cjk_char};

mod snapshot;
pub use snapshot::{RecordHit, SnapshotIndex};

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
struct Hit<'a, T> {
    pub reference: &'a T,
    pub score: u32,
    /// Fields that matched, strongest first, so the UI can say why.
    pub matched: Vec<MatchField>,
    /// Some term matched only with one typo, so the UI can say "close match".
    pub typo: bool,
    /// Some terms did not match at all: one of the closest matches, given
    /// only when nothing matches every term.
    pub partial: bool,
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
    /// Add a record's fields in [`MatchField::ALL`] order. Each of `extras`
    /// (a field's index and its raw text, such as the title itself, a short
    /// title or a journal abbreviation) adds its [`spelling_aliases`] to that
    /// field, and its own words too unless they are already there; the title
    /// proper ends before them. Two spaces set each addition apart, so a
    /// quoted phrase never runs from one into the next. Text past 4 GiB in
    /// either buffer is left out, which no real library comes near.
    fn push(&mut self, mut fields: [String; 8], extras: &[(usize, &str)]) {
        let title_len = fields[1].len();
        for &(field, extra) in extras {
            for extra in [normalize(extra), spelling_aliases(extra)] {
                if !extra.is_empty() && !format!(" {} ", fields[field]).contains(&format!(" {extra} ")) {
                    push_apart(&mut fields[field], &extra);
                }
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

/// Add `words` to `text` two spaces apart, so no phrase spans the two.
fn push_apart(text: &mut String, words: &str) {
    if !words.is_empty() {
        text.push_str("  ");
        text.push_str(words);
    }
}

fn push_words(text: &mut String, words: &str) {
    if words.is_empty() {
        return;
    }
    if !text.is_empty() {
        text.push(' ');
    }
    text.push_str(words);
}

/// Every name, normalised, followed by its given-name initials when there
/// are several ("jrr" for J. R. R. or John Ronald Reuel), so `jrr tolkien`
/// finds it as well as `j tolkien`.
fn name_text<'a>(names: impl Iterator<Item = &'a Name>) -> String {
    let mut text = String::new();
    // Other spellings go after every name, apart, so no phrase spans names.
    let mut aliases = String::new();
    for name in names {
        let raw = name.display();
        let display = normalize(&raw);
        push_words(&mut text, &display);
        push_apart(&mut aliases, &spelling_aliases(&raw));
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
    text.push_str(&aliases);
    text
}

/// The words of `text` that are also spelt another way, spelt that way and
/// normalised, two spaces apart: German and Nordic letters as written
/// without them (Müller as mueller, Ångström as aangstroem) and Greek letters
/// used as symbols by their names (β-amyloid as beta amyloid, NF-κB as nf
/// kappab, "α and β" as alpha and beta). Accents are already folded by
/// [`normalize`], so "muller" finds Müller without this. Greek words are left
/// Greek, and μ before another letter is a unit (μm), not mu. Empty for most
/// text.
fn spelling_aliases(text: &str) -> String {
    if text.is_ascii() {
        return String::new();
    }
    let greek = |c: char| matches!(c, '\u{0370}'..='\u{03FF}' | '\u{1F00}'..='\u{1FFF}' | 'µ');
    let chars: Vec<char> = text.nfc().collect();
    let words: Vec<&[char]> = chars
        .split(|c| c.is_whitespace())
        .filter(|word| !word.is_empty())
        .collect();
    // Letters and digits between punctuation, so α/β is two symbols.
    let pieces = |word: &'_ [char]| -> Vec<(usize, usize)> {
        let mut start = 0;
        word.split(|c| !c.is_alphanumeric())
            .map(|piece| {
                let span = (start, start + piece.len());
                start = span.1 + 1;
                span
            })
            .collect()
    };
    let greek_count = |piece: &[char]| piece.iter().filter(|&&c| greek(c)).count();
    let only_greek = |piece: &[char]| piece.iter().all(|&c| greek(c));
    // A piece of two or more Greek letters and nothing else makes the text
    // Greek, so its one-letter words are Greek words too (Η γη), not symbols.
    let greek_text = words.iter().any(|word| {
        pieces(word)
            .into_iter()
            .any(|(start, end)| only_greek(&word[start..end]) && greek_count(&word[start..end]) > 1)
    });
    let greek_word = |c: char| matches!(c.to_lowercase().next(), Some('ο' | 'η' | 'ω'));
    let mut aliases = String::new();
    for word in words {
        // A lone Greek letter is a symbol, except the one-letter Greek words
        // (ο, η, ω) in Greek text; so is any in a piece with other letters.
        let mut symbols = vec![false; word.len()];
        for (start, end) in pieces(word) {
            let piece = &word[start..end];
            symbols[start..end]
                .fill(!only_greek(piece) || greek_count(piece) == 1 && !(greek_text && greek_word(piece[0])));
        }
        let mut alias = String::new();
        let mut changed = false;
        for (i, &c) in word.iter().enumerate() {
            let next = word.get(i + 1).copied();
            let beside_greek = i > 0 && greek(word[i - 1]) || next.is_some_and(greek);
            let unit = matches!(c, 'μ' | 'µ') && next.is_some_and(char::is_alphabetic);
            let symbol = symbols[i] && !beside_greek && !unit;
            let spelt = match c.to_lowercase().next().unwrap_or(c) {
                'ä' => "ae",
                'ö' | 'ø' => "oe",
                'ü' => "ue",
                'å' => "aa",
                c if !symbol => {
                    alias.push(c);
                    continue;
                }
                'α' => "alpha",
                'β' => "beta",
                'γ' => "gamma",
                'δ' => "delta",
                'ε' | 'ϵ' => "epsilon",
                'ζ' => "zeta",
                'η' => "eta",
                'θ' | 'ϑ' => "theta",
                'ι' => "iota",
                'κ' => "kappa",
                'λ' => "lambda",
                'μ' | 'µ' => "mu",
                'ν' => "nu",
                'ξ' => "xi",
                'ο' => "omicron",
                'π' => "pi",
                'ρ' => "rho",
                'σ' | 'ς' => "sigma",
                'τ' => "tau",
                'υ' => "upsilon",
                'φ' | 'ϕ' => "phi",
                'χ' => "chi",
                'ψ' => "psi",
                'ω' => "omega",
                c => {
                    alias.push(c);
                    continue;
                }
            };
            alias.push_str(spelt);
            changed = true;
        }
        if changed {
            push_apart(&mut aliases, &normalize(&alias));
        }
    }
    aliases.trim_start().to_owned()
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
                partial: false,
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
    /// How many query terms matched; all of them except in [`Pass::Closest`],
    /// which counts only words.
    terms: usize,
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
    /// Only when nothing else matched: records matching at least half the
    /// words (terms that are not numbers) as typed, most words first, as
    /// Meilisearch drops query words when nothing matches them all.
    Closest,
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
    let raw = query;
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
                terms: query.terms.len(),
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
    // Repeated words count each time, so a long pasted text of a few words
    // repeated is still long.
    let typed_words = query
        .terms
        .iter()
        .fold(0_usize, |sum, term| sum.saturating_add(term.count as usize));
    let short_query = typed_words <= MAX_TYPO_QUERY_TERMS;
    let typos = candidates.len() < limit && short_query;
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

    // Nothing matched every term: the records matching the most of its
    // words. Hinted terms (`a:`, `#tag`…) must still match. Numbers and one-
    // or two-letter words are not counted, so a missing page is not matched by
    // its digits, and a DOI or other identifier nobody carries gets no closest
    // matches. Whether this pass runs does not depend on the limit, since
    // every earlier pass runs in full when nothing has been found. Long pasted
    // queries are left out, as for typos.
    let (hinted, words): (Vec<usize>, Vec<usize>) = all.iter().partition(|&&t| query.terms[t].fields != ANY_FIELD);
    // Each counted word, weighted by how many times it was typed: a word typed
    // twice counts twice, the same whether or not it is still being typed.
    let words: Vec<usize> = words
        .into_iter()
        .filter(|&t| !query.terms[t].starts_only && query.terms[t].text.chars().any(char::is_alphabetic))
        .collect();
    let distinct = words
        .iter()
        .map(|&t| query.terms[t].text.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    // A word longer than any a person types also marks pasted text.
    let typed = raw.len() <= 512 && query.terms.iter().all(|term| term.chars.len() <= MAX_TYPO_TERM);
    if candidates.is_empty() && short_query && typed && distinct >= 2 && identifier::parse_query(raw).is_none() {
        // Each record's words matched, score and fields; `None` when the hints leave it out.
        let mut partial: Vec<Option<(usize, u32, u8)>> = vec![hinted.is_empty().then_some((0, 0, 0)); docs.len()];
        if !hinted.is_empty() {
            for (n, score, matched) in matching(&query, docs, texts, &hinted, false, scope, &found, &mut packed, stop)?
            {
                partial[n] = Some((0, score, matched));
            }
        }
        // Each word's match includes the year's score; count it once per record.
        let year_score = if query.years.is_some() {
            MatchField::Year.weight() * WHOLE_WORD
        } else {
            0
        };
        for &t in &words {
            let typed = query.terms[t].count as usize;
            for (n, score, matched) in matching(&query, docs, texts, &[t], false, scope, &found, &mut packed, stop)? {
                if let Some(record) = &mut partial[n] {
                    let score = score.wrapping_sub(year_score);
                    *record = (
                        record.0.saturating_add(typed),
                        weighted_score(record.1, score, 1),
                        record.2 | matched,
                    );
                }
            }
        }
        // At least half the words typed, repeats included.
        let needed = words
            .iter()
            .fold(0_usize, |sum, &t| sum.saturating_add(query.terms[t].count as usize))
            .div_ceil(2);
        for (n, record) in partial.iter().enumerate() {
            if let &Some((terms, score, matched)) = record
                && terms >= needed
            {
                let mut closest = candidate(&query, &docs[n], texts, n, Pass::Closest, score, matched);
                closest.terms = terms;
                candidates.push(closest);
            }
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
            .then(b.terms.cmp(&a.terms))
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
                partial: candidate.pass == Pass::Closest,
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
        terms: query.terms.len(),
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
    use crate::contract::{CitationKey, CitationKeySource, ItemRef, LibraryId, LibraryObject, Record, Snapshot};
    use serde_json::json;
    use std::sync::Arc;

    fn library_id(name: &str) -> LibraryId {
        LibraryId::file(name.as_bytes())
    }

    fn build_index(records: Vec<Record>) -> SnapshotIndex {
        SnapshotIndex::new(Arc::new(
            Snapshot::new(records.into_iter().map(LibraryObject::Record).collect())
                .unwrap_or_else(|error| panic!("{error}")),
        ))
    }

    fn reference(key: &str, title: &str, family: &str, year: i32) -> Record {
        Record {
            id: ItemRef {
                library: library_id("t"),
                key: key.into(),
            },
            citation_keys: vec![CitationKey {
                value: key.into(),
                source: CitationKeySource::Bibtex,
            }],
            aliases: vec![],
            identifiers: vec![],
            csl: serde_json::from_value(json!({
                "type": "article-journal", "title": title,
                "author": [{"family": family}], "issued": {"date-parts": [[year]]}
            }))
            .unwrap_or_else(|error| panic!("{error}")),
        }
    }

    #[test]
    fn container_search_ignores_rich_text_markup_and_entities() {
        let mut r = reference("k", "Title", "Smith", 2020);
        r.csl.insert("container-title".into(), json!("<i>Q</i> &amp; A"));
        let refs = [r];
        let index = build_index(refs.into());
        for query in ["q & a", "Q A", "q&a"] {
            let hits = index.search(query, 10, None, &|| false).unwrap_or_default();
            assert_eq!(hits.len(), 1, "{query}");
            assert!(hits[0].matched.contains(&MatchField::Container), "{query}");
        }
        assert!(index.search("amp", 10, None, &|| false).unwrap_or_default().is_empty());
    }

    #[test]
    fn source_scopes_filter_before_limits_for_words_unicode_and_identifiers() {
        let mut first = reference("same", "García 研究", "Example", 2026);
        first.id.library = library_id("zotero:user");
        for (name, value) in [
            ("DOI", json!("10.1000/scope")),
            ("ISBN", json!("0-306-40615-2")),
            ("PMID", json!("12345")),
        ] {
            first.csl.insert(name.into(), value);
        }
        first.identifiers.push(Identifier::Arxiv("2101.00001".into()));
        let mut refs = vec![first];
        for n in 0..3 {
            let mut r = refs[0].clone();
            r.id.library = library_id("zotero:group:7");
            r.id.key = format!("group{n}");
            r.csl.insert("issued".into(), json!({"date-parts": [[2020 + n]]}));
            refs.push(r);
        }
        let owned = build_index(refs.clone());
        let group = vec![library_id("zotero:group:7")];
        let all = vec![library_id("zotero:user"), library_id("zotero:group:7")];
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
                Some(&[library_id("missing")][..]),
            ] {
                let expected: Vec<_> = owned
                    .search(query, 10, None, &|| false)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|h| scope.is_none_or(|s| s.contains(&h.item.object.id().library)))
                    .take(2)
                    .map(|h| h.item.object.id().clone())
                    .collect();
                let hits = owned.search(query, 2, scope, &|| false).unwrap_or_default();
                assert_eq!(
                    hits.iter().map(|h| h.item.object.id().clone()).collect::<Vec<_>>(),
                    expected
                );
            }
            assert!(
                owned
                    .search(query, 0, Some(&group), &|| false)
                    .unwrap_or_default()
                    .is_empty()
            );
            assert!(owned.search(query, 2, Some(&group), &|| true).is_none());
        }
        assert!(
            owned
                .search("", 2, Some(&group), &|| false)
                .unwrap_or_default()
                .is_empty()
        );
        // An identifier outside this scope must not suppress an ordinary match inside it.
        let mut fallback = reference("fallback", "10.1000/scope", "Example", 2020);
        fallback.id.library = library_id("file");
        refs.push(fallback);
        let owned = build_index(refs);
        let hits = owned
            .search("10.1000/scope", 1, Some(&[library_id("file")]), &|| false)
            .unwrap_or_default();
        assert_eq!(hits[0].item.object.id().key, "fallback");
        assert_eq!(hits[0].matched, [MatchField::Title]);
    }

    #[test]
    fn scoped_search_cancels_when_a_concurrent_request_supersedes_it() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        let mut r = reference("a", "Synthetic research", "Example", 2020);
        r.csl.insert("DOI".into(), json!("10.1000/scope"));
        let index = build_index(
            (0..10_000)
                .map(|n| {
                    let mut r = r.clone();
                    r.id.key = format!("item{n}");
                    r
                })
                .collect(),
        );
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
                    index.search(query, 20, Some(&[library_id("excluded")]), &stop)
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

    #[test]
    fn accents_and_case_are_ignored() {
        let refs = [reference("garcia2022", "Sueño y memoria", "García", 2022)];
        let index = build_index(refs.into());
        let hits = index.search("GARCIA sueno", 10, None, &|| false).unwrap_or_default();
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
        let index = build_index(refs.into());
        let hits = index.search("sleep memory", 10, None, &|| false).unwrap_or_default();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].item.object.id().key, "a");
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
            let index = build_index(vec![r]);
            let parse_checks = Cell::new(0);
            assert!(
                Query::parse(query, &|| {
                    parse_checks.set(parse_checks.get() + 1);
                    false
                })
                .is_some()
            );
            // Allow the identifier probe and query preparation, then cancel
            // while processing one matching record, without relying on time.
            let checks = Cell::new(0);
            assert!(
                index
                    .search(query, 10, None, &|| {
                        checks.set(checks.get() + 1);
                        checks.get() > parse_checks.get() + 2
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
        red.csl.insert("abstract".into(), json!("Blue"));
        let mut blue = reference("b", "Blue", "Example", 2025);
        blue.csl.insert("abstract".into(), json!("Red"));
        let refs = [blue, red];
        let owned = build_index(refs.into());
        let hits = owned.search("RÉD red blue", 10, None, &|| false).unwrap_or_default();
        assert_eq!(
            hits.iter()
                .map(|h| (&*h.item.object.id().key, h.score, h.matched.clone()))
                .collect::<Vec<_>>(),
            [
                ("r", 84, vec![MatchField::Title, MatchField::Abstract]),
                ("b", 48, vec![MatchField::Title, MatchField::Abstract]),
            ]
        );
        let hits = owned
            .search("red red blue", 1, Some(&[library_id("t")]), &|| false)
            .unwrap_or_default();
        assert_eq!(hits[0].item.object.id().key, "r");
        assert!(
            owned
                .search("red red blue", 1, Some(&[]), &|| false)
                .unwrap_or_default()
                .is_empty()
        );
        // Coalescing terms must not turn a repeated query into an exact match.
        assert_eq!(
            owned.search("red red", 1, None, &|| false).unwrap_or_default()[0].score,
            80
        );
        let exact = build_index(vec![reference("red red", "red red", "Example", 2024)]);
        assert_eq!(
            exact.search("red red", 1, None, &|| false).unwrap_or_default()[0].score,
            2096
        );
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
        let index = build_index(
            (0..5_000)
                .map(|n| reference(&format!("key{n}"), "Synthetic research", "Example", 2024))
                .collect(),
        );
        for query in ["research".to_owned(), "research ".repeat(1_000)] {
            let mut elapsed = Duration::ZERO;
            for _ in 0..10 {
                let started = Instant::now();
                let hits = index.search(&query, 20, None, &|| false).unwrap_or_default();
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
        let index = build_index(refs.into());
        assert_eq!(
            index.search("smith2020", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "smith2020"
        );
        assert!(
            index
                .search("202", 10, None, &|| false)
                .unwrap_or_default()
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
        let index = build_index(refs.into());
        assert_eq!(
            index.search("γνωσις", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "gr"
        );
        assert_eq!(
            index.search("ΓΝΏΣΙΣ", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "gr"
        );
        assert_eq!(
            index.search("strasse weiss", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "de"
        );
    }

    #[test]
    fn exact_key_match_is_exact_not_punctuation_folded() {
        let refs = [
            reference("a:b", "Newer", "X", 2024),
            reference("a-b", "Older", "Y", 2001),
        ];
        let index = build_index(refs.into());
        assert_eq!(
            index.search("a-b", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "a-b"
        );
        assert_eq!(
            index.search("a:b", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "a:b"
        );
    }

    #[test]
    fn translators_and_other_roles_are_searchable() {
        let mut translation = reference("translation", "Synthetic", "Example", 2024);
        translation
            .csl
            .insert("translator".into(), json!([{"family":"Translator","given":"Taylor"}]));
        let mut compilation = reference("compilation", "Another", "Example", 2024);
        compilation
            .csl
            .insert("compiler".into(), json!([{"family":"Compiler","given":"Casey"}]));
        let refs = vec![translation, compilation];
        let index = build_index(refs);
        assert_eq!(
            index
                .search("Translator", 10, None, &|| false)
                .unwrap_or_default()
                .len(),
            1
        );
        assert_eq!(
            index.search("Compiler", 10, None, &|| false).unwrap_or_default().len(),
            1
        );
    }

    #[test]
    fn cjk_and_arabic_substrings_match() {
        let refs = [
            reference("zh", "睡眠与记忆", "示例", 2025),
            reference("ar", "النوم والذاكرة", "مثال", 2021),
        ];
        let index = build_index(refs.into());
        assert_eq!(
            index.search("记忆", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "zh"
        );
        assert_eq!(
            index.search("الذاكرة", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "ar"
        );
    }

    #[test]
    fn latin_letters_without_decomposition_match_their_plain_forms() {
        let refs = [
            reference("pl", "Historia Łodzi", "Kowalczyk", 2020),
            reference("no", "Søvn og hukommelse", "Ødegård", 2021),
            reference("fr", "Œuvres et Cæsar", "Đorđević", 2022),
            reference("tr", "Uykunun ışığı", "Yılmaz", 2023),
        ];
        let index = build_index(refs.into());
        assert_eq!(
            index.search("lodzi", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "pl"
        );
        assert_eq!(
            index.search("sovn odegard", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "no"
        );
        assert_eq!(
            index
                .search("oeuvres caesar dordevic", 10, None, &|| false)
                .unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "fr"
        );
        assert_eq!(
            index.search("isigi yilmaz", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "tr"
        );
    }

    #[test]
    fn persian_and_arabic_letter_variants_and_tatweel_match() {
        // Arabic kaf and yeh as stored; Persian keyboards type ک and ی.
        let refs = [
            reference("fa", "كتاب الذكي", "علي", 2020),
            reference("tw", "العـــلم", "مثال", 2021),
        ];
        let index = build_index(refs.into());
        assert_eq!(
            index.search("کتاب", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "fa"
        );
        assert_eq!(
            index.search("علی", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "fa"
        );
        assert_eq!(
            index.search("العلم", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "tw"
        );
    }

    #[test]
    fn hiragana_and_katakana_match_each_other() {
        let refs = [reference("ja", "ヤマダの研究", "ヤマダ", 2020)];
        let index = build_index(refs.into());
        assert_eq!(index.search("やまだ", 10, None, &|| false).unwrap_or_default().len(), 1);
        assert_eq!(index.search("ﾔﾏﾀﾞ", 10, None, &|| false).unwrap_or_default().len(), 1);
    }

    #[test]
    fn marks_that_change_letters_in_other_scripts_are_kept() {
        let refs = [
            reference("ga", "がっこう", "例", 2020),
            reference("ka", "かっこう", "例", 2021),
            reference("hi", "किताब", "उदाहरण", 2022),
        ];
        let index = build_index(refs.into());
        let keys = |q| {
            index
                .search(q, 10, None, &|| false)
                .unwrap_or_default()
                .iter()
                .map(|h| h.item.object.id().key.clone())
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
        let index = build_index(refs.into());
        assert_eq!(
            index.search("葛城", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "vs"
        );
        assert_eq!(
            index.search("ספר", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "he"
        );
        assert_eq!(normalize("❤\u{FE0F}x"), "x");
        assert_eq!(normalize("か゛ き"), "か き");
        assert_eq!(normalize("בית־ספר"), "בית ספר");

        let refs = [reference("ko", "한국어 연구", "김", 2020)];
        let index = build_index(refs.into());
        assert_eq!(index.search("한국", 10, None, &|| false).unwrap_or_default().len(), 1);
        assert!(index.search("하국", 10, None, &|| false).unwrap_or_default().is_empty());
    }

    #[test]
    fn a_stopped_search_returns_nothing_and_limits_keep_the_best() {
        let refs: Vec<_> = (0..10_000)
            .map(|n| reference(&format!("k{n}"), "Sleep study", "Smith", 2000 + (n % 20)))
            .collect();
        let index = build_index(refs);
        assert!(index.search("sleep", 10, None, &|| true).is_none());
        assert!(index.search("sleep", 0, None, &|| false).unwrap_or_default().is_empty());
        let top = index.search("sleep", 5, None, &|| false).unwrap_or_default();
        let all = index.search("sleep", 10_000, None, &|| false).unwrap_or_default();
        assert_eq!(all.len(), 10_000);
        let ids = |hits: &[RecordHit<'_>]| hits.iter().map(|h| h.item.object.id().clone()).collect::<Vec<_>>();
        assert_eq!(ids(&top), ids(&all[..5]));
        assert_eq!(top[0].matched, [MatchField::Title]);
    }

    #[test]
    fn exact_keys_with_greek_sigma_match_in_any_case() {
        let refs = [
            reference("ΜΑΣ", "Other", "X", 2020),
            reference("μας13", "Other", "Y", 2021),
        ];
        let index = build_index(refs.into());
        for (query, key) in [("μας", "ΜΑΣ"), ("Μας", "ΜΑΣ"), ("ΜΑΣ13", "μας13")] {
            let hits = index.search(query, 10, None, &|| false).unwrap_or_default();
            assert_eq!(hits[0].item.object.id().key, key, "{query}");
            assert!(hits[0].score >= 2000, "{query}");
        }
    }

    #[test]
    fn word_start_matches_score_higher_than_inner_matches() {
        let refs = [
            reference("a", "Insomnia", "X", 2020),
            reference("b", "The somnia", "Y", 2020),
        ];
        let index = build_index(refs.into());
        let hits = index.search("somnia", 10, None, &|| false).unwrap_or_default();
        assert_eq!(hits[0].item.object.id().key, "b");
        assert_eq!(hits[0].score, 2 * hits[1].score);
    }

    #[test]
    fn a_pasted_identifier_finds_the_record_that_carries_it() {
        let mut doi = reference("doi1", "Sleep", "Example", 2020);
        doi.csl.insert("DOI".into(), json!("10.1000/ABC.1"));
        let mut isbn = reference("isbn1", "Memory", "Example", 2019);
        isbn.csl.insert("ISBN".into(), json!("0-306-40615-2"));
        let mut arxiv = reference("arx", "Dreams", "Example", 2021);
        arxiv
            .csl
            .insert("URL".into(), json!("https://arxiv.org/abs/2101.00001v2"));
        let mut pmid = reference("pm", "Naps", "Example", 2022);
        pmid.csl
            .insert("URL".into(), json!("https://pubmed.ncbi.nlm.nih.gov/12345/"));
        let mut old = reference("old", "Strings", "Example", 1997);
        old.identifiers.push(Identifier::Arxiv("hep-th/9711200".into()));
        let refs = vec![
            doi,
            isbn,
            arxiv,
            pmid,
            old,
            reference("other", "Mentions 10.1000/abc.1 in passing", "Example", 2023),
        ];
        let owned = build_index(refs);
        for (query, key, field) in [
            ("https://doi.org/10.1000/abc.1", "doi1", MatchField::Doi),
            ("978-0-306-40615-7", "isbn1", MatchField::Identifier),
            ("arXiv:2101.00001", "arx", MatchField::Identifier),
            ("PMID:12345", "pm", MatchField::Identifier),
            ("hep-th/9711200", "old", MatchField::Identifier),
            ("arXiv:hep-th/9711200", "old", MatchField::Identifier),
            ("https://arxiv.org/abs/hep-th/9711200", "old", MatchField::Identifier),
        ] {
            let hits = owned.search(query, 10, None, &|| false).unwrap_or_default();
            assert_eq!(hits.len(), 1, "{query}");
            assert_eq!(hits[0].item.object.id().key, key, "{query}");
            assert_eq!(hits[0].matched, [field], "{query}");
        }
        // An identifier nobody carries falls back to ordinary search.
        assert_eq!(
            owned
                .search("10.1000/zzz", 10, None, &|| false)
                .unwrap_or_default()
                .len(),
            0
        );
        assert_eq!(
            owned.search("passing", 10, None, &|| false).unwrap_or_default()[0]
                .item
                .object
                .id()
                .key,
            "other"
        );
    }

    /// A small synthetic library for the forgiving-search rules below.
    fn library() -> SnapshotIndex {
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
        refs[0].csl.insert(
            "author".into(),
            json!([{"family":"Smith","given":"John"},{"family":"Jones","given":"Ann"}]),
        );
        refs[0].csl.insert("keyword".into(), json!("sleep, memory"));
        refs[0]
            .csl
            .insert("container-title".into(), json!("Journal of Sleep Research"));
        refs[0].csl.insert("container-title-short".into(), json!("J Sleep Res"));
        refs[1].csl.insert("keyword".into(), json!("sleepiness"));
        refs[8]
            .csl
            .insert("author".into(), json!([{"family":"Tolkien","given":"J. R. R."}]));
        refs[9].csl.insert("author".into(), json!([{"literal":"山田 太郎"}]));
        refs[10]
            .csl
            .insert("abstract".into(), json!("A study of sleep and memory in rats"));
        build_index(refs)
    }

    fn keys(index: &SnapshotIndex, query: &str, limit: usize) -> Vec<String> {
        index
            .search(query, limit, None, &|| false)
            .unwrap_or_default()
            .iter()
            .map(|hit| hit.item.object.id().key.clone())
            .collect()
    }

    #[test]
    fn one_typo_is_forgiven_after_every_exact_match() {
        let index = library();
        // Completed words: one letter swapped, changed, missing or extra.
        for query in ["smtih ", "smiht ", "smitj ", "smithh ", "consolidaton "] {
            assert_eq!(keys(&index, query, 10), ["smith2020"], "{query:?}");
            assert!(
                index.search(query, 10, None, &|| false).unwrap_or_default()[0].typo,
                "{query:?}"
            );
        }
        // A word still being typed may be a misspelt start of a word.
        assert_eq!(keys(&index, "nueros", 10), ["li2021"]);
        assert!(keys(&index, "nueros ", 10).is_empty());
        // Exact matches come first; a close match only follows them.
        assert_eq!(keys(&index, "trial ", 10), ["trial2016", "trail2015"]);
        assert_eq!(keys(&index, "trial ", 1), ["trial2016"]);
        assert!(!index.search("trial ", 1, None, &|| false).unwrap_or_default()[0].typo);
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
        assert_eq!(
            index.search("sleep memory", 10, None, &|| false).unwrap_or_default()[1].matched,
            [MatchField::Abstract]
        );
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
        tagged.csl.insert("keyword".into(), json!("sleepiness"));
        let mut abbreviated = reference("rna", "MicroRNA", "Example", 2019);
        abbreviated.csl.insert("title-short".into(), json!("RNA"));
        let mut keyword_only = reference("kw", "Other", "Example", 2018);
        keyword_only.csl.insert("keyword".into(), json!("sleep"));
        let index = build_index(vec![
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
        first.csl.insert("abstract".into(), json!("foo foo"));
        let index = build_index(vec![first, reference("bar", "foo foo stuff", "Example", 2020)]);
        assert_eq!(keys(&index, "\"foo foo\"", 10), ["foo", "bar"]);
        assert_eq!(keys(&index, "\"foo foo\"", 1), ["foo"]);
    }

    #[test]
    fn hostile_queries_do_bounded_work_and_typos_poll_cancellation() {
        use std::cell::Cell;
        let index = build_index(
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
                .search(&words, 20, None, &stop)
                .is_some_and(|hits| hits.is_empty())
        );
        // Parsing polls every 64 words; nothing rescans the index per word.
        assert!(checks.get() < 1_000, "{}", checks.get());

        let alphas = vec!["α"; 50_000].join(" ");
        let index = build_index(vec![reference("greek", &alphas, "Example", 2020)]);
        checks.set(0);
        assert!(
            index
                .search("αβγδε ", 20, None, &stop)
                .is_some_and(|hits| hits.is_empty())
        );
        assert!(checks.get() > 10, "the typo pass polls while it scans");
    }
}
