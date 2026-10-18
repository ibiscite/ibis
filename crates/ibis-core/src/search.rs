//! Instant keyword search with an explanation for every match.
//!
//! Matching ignores case and accents ("garcia" finds "García") and works for
//! any script, including CJK, through substring matching. Every query term
//! must match some field. Exact citation-key and exact title matches rank
//! first. This in-memory index is the 0.x baseline; a persistent SQLite FTS5
//! index replaces it when libraries become large.

use serde::Serialize;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::model::Reference;

/// Which part of a record matched a query term.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
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
    docs: Vec<(&'a Reference, [String; 8])>,
}

impl<'a> Index<'a> {
    pub fn new(references: &'a [Reference]) -> Self {
        Index {
            docs: references.iter().map(|r| (r, prepare(r))).collect(),
        }
    }

    /// Return up to `limit` matches, best first. An empty query matches nothing.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit<'a>> {
        rank(self.docs.iter().map(|(r, f)| (*r, f)), query, limit)
    }
}

/// Search index that owns its references, for front ends that keep a library
/// open and search it many times. Normalization happens once, when it is built.
#[derive(Default)]
pub struct OwnedIndex {
    references: Vec<Reference>,
    fields: Vec<[String; 8]>,
}

impl OwnedIndex {
    pub fn new(references: Vec<Reference>) -> Self {
        let fields = references.iter().map(prepare).collect();
        OwnedIndex { references, fields }
    }

    pub fn references(&self) -> &[Reference] {
        &self.references
    }

    /// Return up to `limit` matches, best first. An empty query matches nothing.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit<'_>> {
        rank(self.references.iter().zip(&self.fields), query, limit)
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
        normalize(r.container_title.as_deref().unwrap_or("")),
        normalize(r.abstract_text.as_deref().unwrap_or("")),
    ]
}

fn rank<'r, 'f>(
    docs: impl Iterator<Item = (&'r Reference, &'f [String; 8])>,
    query: &str,
    limit: usize,
) -> Vec<Hit<'r>> {
    let normalized = normalize(query);
    let terms: Vec<&str> = normalized.split_whitespace().collect();
    if terms.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<Hit<'r>> = docs
        .filter_map(|(reference, fields)| {
            let mut score = 0;
            let mut matched = Vec::new();
            for term in &terms {
                let best = MatchField::ALL
                    .iter()
                    .zip(fields.iter())
                    .filter(|(field, text)| match field {
                        MatchField::Year => text.as_str() == *term,
                        _ => text.contains(term),
                    })
                    .map(|(field, text)| (*field, field.weight() * if starts_word(text, term) { 2 } else { 1 }))
                    .max_by_key(|(_, w)| *w)?;
                score += best.1;
                if !matched.contains(&best.0) {
                    matched.push(best.0);
                }
            }
            let exact_key = reference
                .citekey
                .as_deref()
                .is_some_and(|k| k.to_lowercase() == query.trim().to_lowercase());
            if exact_key {
                score += 2000;
            } else if fields[0] == normalized.trim() {
                score += 1000;
            } else if fields[1] == normalized.trim() {
                score += 500;
            }
            matched.sort();
            Some(Hit {
                reference,
                score,
                matched,
            })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.reference.year().cmp(&a.reference.year()))
            .then_with(|| a.reference.id.cmp(&b.reference.id))
    });
    hits.truncate(limit);
    hits
}

/// Lowercase, strip accents, fold letter variants and turn punctuation into spaces.
///
/// Folding covers letters whose lowercase form depends on position or that
/// have common alternative spellings: Greek final sigma (ς → σ), German
/// sharp s (ß → ss) and long s (ſ → s).
pub fn normalize(text: &str) -> String {
    let folded: String = text
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .flat_map(|c| match c {
            'ς' => ['σ', '\0'],
            'ß' => ['s', 's'],
            'ſ' => ['s', '\0'],
            c if c.is_alphanumeric() => [c, '\0'],
            _ => [' ', '\0'],
        })
        .filter(|c| *c != '\0')
        .collect();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn starts_word(text: &str, term: &str) -> bool {
    text.starts_with(term) || text.contains(&format!(" {term}"))
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
            variables: Default::default(),
            raw: None,
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
