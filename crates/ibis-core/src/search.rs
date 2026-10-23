//! Instant keyword search with an explanation for every match.
//!
//! Matching ignores case and accents ("garcia" finds "García") and works for
//! any script, including CJK, through substring matching. Every query term
//! must match some field. Exact citation-key and exact title matches rank
//! first. This in-memory index is the 0.x baseline; a persistent SQLite FTS5
//! index replaces it when libraries become large.

use std::collections::BTreeMap;

use serde::Serialize;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::identifier::{self, Identifier};
use crate::model::{Reference, strip_markup};

/// Which part of a record matched a query term.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
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
}

/// One search result.
#[derive(Debug, Clone, Serialize)]
pub struct Hit<'a> {
    pub reference: &'a Reference,
    pub score: u32,
    /// Fields that matched, strongest first, so the UI can say why.
    pub matched: Vec<MatchField>,
}

/// Search index over a set of references.
pub struct Index<'a> {
    docs: Vec<(&'a Reference, [String; 8], Vec<Identifier>)>,
}

impl<'a> Index<'a> {
    pub fn new(references: &'a [Reference]) -> Self {
        Index {
            docs: references
                .iter()
                .map(|r| (r, prepare(r), identifier::of_reference(r)))
                .collect(),
        }
    }

    /// Return up to `limit` matches, best first. An empty query matches nothing.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit<'a>> {
        self.search_scoped(query, limit, None)
    }

    /// Search only these exact source IDs before ranking or limiting results.
    /// `None` searches all sources; an empty slice searches none.
    pub fn search_scoped(&self, query: &str, limit: usize, scope: Option<&[String]>) -> Vec<Hit<'a>> {
        if let Some(hits) = by_identifier(
            self.docs.iter().map(|(r, _, ids)| (*r, ids.as_slice())),
            query,
            limit,
            scope,
            &|| false,
        )
        .flatten()
        {
            return hits;
        }
        rank(self.docs.iter().map(|(r, f, _)| (*r, f)), query, limit, scope, &|| {
            false
        })
        .unwrap_or_default()
    }
}

/// Search index that owns its references, for front ends that keep a library
/// open and search it many times. Normalization happens once, when it is built.
#[derive(Default)]
pub struct OwnedIndex {
    references: Vec<Reference>,
    fields: Vec<[String; 8]>,
    identifiers: Vec<Vec<Identifier>>,
    source_counts: BTreeMap<String, usize>,
}

impl OwnedIndex {
    pub fn new(references: Vec<Reference>) -> Self {
        let fields = references.iter().map(prepare).collect();
        let mut source_counts = BTreeMap::new();
        for reference in &references {
            *source_counts.entry(reference.id.source.clone()).or_default() += 1;
        }
        let identifiers = references.iter().map(identifier::of_reference).collect();
        OwnedIndex {
            references,
            fields,
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

    /// Like [`Index::search_scoped`], using this owned snapshot.
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
        rank(self.references.iter().zip(&self.fields), query, limit, scope, stop)
    }
}

/// Normalized text of each [`MatchField`], in the order of [`MatchField::ALL`].
fn prepare(r: &Reference) -> [String; 8] {
    let names = r
        .authors
        .iter()
        .chain(&r.editors)
        .chain(r.other_names.values().flatten())
        .map(|n| n.display())
        .collect::<Vec<_>>()
        .join(" ");
    [
        normalize(r.citekey.as_deref().unwrap_or("")),
        normalize(&r.title_plain().unwrap_or_default()),
        normalize(&names),
        r.year().map(|y| y.to_string()).unwrap_or_default(),
        normalize(r.doi.as_deref().unwrap_or("")),
        normalize(&r.keywords.join(" ")),
        normalize(&r.container_title.as_deref().map(strip_markup).unwrap_or_default()),
        normalize(r.abstract_text.as_deref().unwrap_or("")),
    ]
}

/// When the whole query is a DOI, ISBN, PubMed ID or arXiv ID that some
/// records in scope carry, those records, newest first. `Some(None)` falls
/// back to ordinary search; `None` means cancellation.
fn by_identifier<'r, 'i>(
    docs: impl Iterator<Item = (&'r Reference, &'i [Identifier])>,
    query: &str,
    limit: usize,
    scope: Option<&[String]>,
    stop: &dyn Fn() -> bool,
) -> Option<Option<Vec<Hit<'r>>>> {
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
            .then_with(|| a.reference.id.cmp(&b.reference.id))
    });
    hits.truncate(limit);
    Some(Some(hits))
}

const STOP_CHECK_INTERVAL: usize = 4096;
const TERM_CHECK_INTERVAL: usize = 64;

fn in_scope(reference: &Reference, scope: Option<&[String]>) -> bool {
    scope.is_none_or(|sources| sources.contains(&reference.id.source))
}

fn rank<'r, 'f>(
    docs: impl Iterator<Item = (&'r Reference, &'f [String; 8])>,
    query: &str,
    limit: usize,
    scope: Option<&[String]>,
    stop: &dyn Fn() -> bool,
) -> Option<Vec<Hit<'r>>> {
    let normalized = normalize(query);
    let mut terms = BTreeMap::<&str, u32>::new();
    for (n, term) in normalized.split_whitespace().enumerate() {
        if n % TERM_CHECK_INTERVAL == 0 && stop() {
            return None;
        }
        let count = terms.entry(term).or_default();
        *count = count.wrapping_add(1);
    }
    if terms.is_empty() || limit == 0 {
        return Some(Vec::new());
    }
    let normalized = normalized.trim();
    // Lowercase character by character, with final sigma folded, so keys are
    // compared without allocating per record.
    let lower = |c: char| char::to_lowercase(c).map(|c| if c == 'ς' { 'σ' } else { c });
    let exact_query: String = query.trim().chars().flat_map(lower).collect();
    // Score every record without allocating; matched fields are a bit set and
    // become a list only for the records that are returned.
    let mut candidates: Vec<(u32, u8, &'r Reference)> = Vec::new();
    let mut term_work = 0;
    for (n, (reference, fields)) in docs.enumerate() {
        if n % STOP_CHECK_INTERVAL == 0 && stop() {
            return None;
        }
        if !in_scope(reference, scope) {
            continue;
        }
        let mut score = 0;
        let mut matched = 0u8;
        let mut all_terms = true;
        for (&term, &count) in &terms {
            term_work += 1;
            if term_work == TERM_CHECK_INTERVAL {
                term_work = 0;
                if stop() {
                    return None;
                }
            }
            let mut best = None;
            for (bit, (field, text)) in MatchField::ALL.iter().zip(fields).enumerate() {
                let at_word_start = match field {
                    MatchField::Year => (text.as_str() == term).then_some(true),
                    _ => find_term(text, term, stop)?,
                };
                if let Some(at_word_start) = at_word_start {
                    let weight = field.weight() * if at_word_start { 2 } else { 1 };
                    // Keep max_by_key's last-field tie preference.
                    if best.is_none_or(|(_, previous)| weight >= previous) {
                        best = Some((bit, weight));
                    }
                }
            }
            let Some((bit, weight)) = best else {
                all_terms = false;
                break;
            };
            score = weighted_score(score, weight, count);
            matched |= 1 << bit;
        }
        if !all_terms {
            continue;
        }
        let exact_key = reference
            .citekey
            .as_deref()
            .is_some_and(|k| k.chars().flat_map(lower).eq(exact_query.chars()));
        if exact_key {
            score = weighted_score(score, 2000, 1);
        } else if fields[0] == normalized {
            score = weighted_score(score, 1000, 1);
        } else if fields[1] == normalized {
            score = weighted_score(score, 500, 1);
        }
        candidates.push((score, matched, reference));
    }
    let order = |a: &(u32, u8, &Reference), b: &(u32, u8, &Reference)| {
        b.0.cmp(&a.0)
            .then_with(|| b.2.year().cmp(&a.2.year()))
            .then_with(|| a.2.id.cmp(&b.2.id))
    };
    if candidates.len() > limit {
        candidates.select_nth_unstable_by(limit - 1, order);
        candidates.truncate(limit);
    }
    candidates.sort_unstable_by(order);
    Some(
        candidates
            .into_iter()
            .map(|(score, bits, reference)| Hit {
                reference,
                score,
                matched: MatchField::ALL
                    .iter()
                    .enumerate()
                    .filter(|(bit, _)| bits & (1 << bit) != 0)
                    .map(|(_, field)| *field)
                    .collect(),
            })
            .collect(),
    )
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

/// Whether `term` occurs in `text`, and if so whether any occurrence starts a word.
fn find_term(text: &str, term: &str, stop: &dyn Fn() -> bool) -> Option<Option<bool>> {
    // `contains` has a vectorized fast path; most fields do not match at all.
    if !text.contains(term) {
        return Some(None);
    }
    let mut found = false;
    for (n, (at, _)) in text.match_indices(term).enumerate() {
        if (n + 1) % STOP_CHECK_INTERVAL == 0 && stop() {
            return None;
        }
        if at == 0 || text.as_bytes()[at - 1] == b' ' {
            return Some(Some(true));
        }
        found = true;
    }
    Some(found.then_some(false))
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
        let index = Index::new(&refs);
        for query in ["q & a", "Q A", "q&a"] {
            let hits = index.search(query, 10);
            assert_eq!(hits.len(), 1, "{query}");
            assert!(hits[0].matched.contains(&MatchField::Container), "{query}");
        }
        assert!(index.search("amp", 10).is_empty());
        assert_eq!(prepare(&refs[0])[6], "q a");
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
        let borrowed = Index::new(&refs);
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
                for hits in [
                    borrowed.search_scoped(query, 2, scope),
                    owned.search_scoped(query, 2, scope),
                ] {
                    assert_eq!(
                        hits.iter().map(|h| h.reference.id.clone()).collect::<Vec<_>>(),
                        expected
                    );
                }
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
        let index = Index::new(&refs);
        let hits = index.search("GARCIA sueno", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].matched, [MatchField::Citekey, MatchField::Title]);
    }

    #[test]
    fn all_terms_must_match() {
        let refs = [
            reference("a", "Sleep and memory", "Smith", 2020),
            reference("b", "Sleep and diet", "Jones", 2021),
        ];
        let index = Index::new(&refs);
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
            let fields = prepare(&r);
            let entered_record = Cell::new(false);
            let checks_in_record = Cell::new(0);
            let docs = std::iter::once((&r, &fields)).inspect(|_| entered_record.set(true));
            // Cancellation becomes visible only after query preparation, while
            // processing one matching record, without relying on elapsed time.
            assert!(
                rank(docs, query, 10, None, &|| {
                    if entered_record.get() {
                        checks_in_record.set(checks_in_record.get() + 1);
                    }
                    checks_in_record.get() > 1
                })
                .is_none()
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
        let index = Index::new(&refs);
        let owned = OwnedIndex::new(refs.to_vec());
        for hits in [index.search("RÉD red blue", 10), owned.search("RÉD red blue", 10)] {
            assert_eq!(
                hits.iter()
                    .map(|h| (&*h.reference.id.native_key, h.score, h.matched.clone()))
                    .collect::<Vec<_>>(),
                [
                    ("r", 42, vec![MatchField::Title, MatchField::Abstract]),
                    ("b", 24, vec![MatchField::Title, MatchField::Abstract]),
                ]
            );
        }
        let hits = owned.search_scoped("red red blue", 1, Some(&["t".into()]));
        assert_eq!(hits[0].reference.id.native_key, "r");
        assert!(owned.search_scoped("red red blue", 1, Some(&[])).is_empty());
        // Coalescing terms must not turn a repeated query into an exact match.
        assert_eq!(owned.search("red red", 1)[0].score, 40);
        let exact = OwnedIndex::new(vec![reference("red red", "red red", "Example", 2024)]);
        assert_eq!(exact.search("red red", 1)[0].score, 2048);
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
        let index = Index::new(&refs);
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
        let index = Index::new(&refs);
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
        let index = Index::new(&refs);
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
        let index = Index::new(&refs);
        assert_eq!(index.search("Translator", 10).len(), 1);
        assert_eq!(index.search("Compiler", 10).len(), 1);
    }

    #[test]
    fn cjk_and_arabic_substrings_match() {
        let refs = [
            reference("zh", "睡眠与记忆", "示例", 2025),
            reference("ar", "النوم والذاكرة", "مثال", 2021),
        ];
        let index = Index::new(&refs);
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
        let index = Index::new(&refs);
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
        let index = Index::new(&refs);
        assert_eq!(index.search("کتاب", 10)[0].reference.id.native_key, "fa");
        assert_eq!(index.search("علی", 10)[0].reference.id.native_key, "fa");
        assert_eq!(index.search("العلم", 10)[0].reference.id.native_key, "tw");
    }

    #[test]
    fn hiragana_and_katakana_match_each_other() {
        let refs = [reference("ja", "ヤマダの研究", "ヤマダ", 2020)];
        let index = Index::new(&refs);
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
        let index = Index::new(&refs);
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
        let index = Index::new(&refs);
        assert_eq!(index.search("葛城", 10)[0].reference.id.native_key, "vs");
        assert_eq!(index.search("ספר", 10)[0].reference.id.native_key, "he");
        assert_eq!(normalize("❤\u{FE0F}x"), "x");
        assert_eq!(normalize("か゛ き"), "か き");
        assert_eq!(normalize("בית־ספר"), "בית ספר");

        let refs = [reference("ko", "한국어 연구", "김", 2020)];
        let index = Index::new(&refs);
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
        let index = Index::new(&refs);
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
        let index = Index::new(&refs);
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
        let borrowed = Index::new(&refs);
        let owned = OwnedIndex::new(refs.clone());
        for (query, key, field) in [
            ("https://doi.org/10.1000/abc.1", "doi1", MatchField::Doi),
            ("978-0-306-40615-7", "isbn1", MatchField::Identifier),
            ("arXiv:2101.00001", "arx", MatchField::Identifier),
            ("PMID:12345", "pm", MatchField::Identifier),
            ("hep-th/9711200", "old", MatchField::Identifier),
            ("arXiv:hep-th/9711200", "old", MatchField::Identifier),
            ("https://arxiv.org/abs/hep-th/9711200", "old", MatchField::Identifier),
        ] {
            for hits in [borrowed.search(query, 10), owned.search(query, 10)] {
                assert_eq!(hits.len(), 1, "{query}");
                assert_eq!(hits[0].reference.id.native_key, key, "{query}");
                assert_eq!(hits[0].matched, [field], "{query}");
            }
        }
        // An identifier nobody carries falls back to ordinary search.
        assert_eq!(owned.search("10.1000/zzz", 10).len(), 0);
        assert_eq!(owned.search("passing", 10)[0].reference.id.native_key, "other");
    }

    #[test]
    fn owned_index_ranks_exactly_like_the_borrowed_one() {
        let refs = vec![
            reference("smith2020", "Sleep and memory", "Smith", 2020),
            reference("smith2021", "Sleep and diet", "Smith", 2021),
            reference("garcia2022", "Sueño y memoria", "García", 2022),
        ];
        let borrowed = Index::new(&refs);
        let owned = OwnedIndex::new(refs.clone());
        for query in ["sleep", "smith", "memoria", "2021", "", "missing"] {
            let a: Vec<_> = borrowed
                .search(query, 10)
                .iter()
                .map(|h| (&h.reference.id, h.score))
                .collect();
            let b: Vec<_> = owned
                .search(query, 10)
                .iter()
                .map(|h| (&h.reference.id, h.score))
                .collect();
            assert_eq!(a, b, "{query}");
        }
        assert_eq!(owned.references().len(), 3);
    }
}
