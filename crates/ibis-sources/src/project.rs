//! The bibliography a project compiles against, chosen by the person and read
//! only to check keys. It is never a search library: loading it as one would
//! give every key a second owner, and could not prove a key absent.
//!
//! Presence is all a check can say. It does not show that the fields match,
//! that the export is current or that the document compiles.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ibis_core::contract::{Fingerprint, LibraryObject, Record};
use ibis_core::model::strip_markup;
use ibis_core::panic_guard;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::SourceError;
use crate::bibtex::project::{self as scan, KeyScan};
use crate::file;

/// Why a read could not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectProblem {
    /// Missing, online-only or not reachable in time.
    Unavailable,
    NotAFile,
    TooLarge,
    NotUtf8,
    Changing,
    /// A project bibliography is BibTeX or BibLaTeX.
    NotBibtex,
}

impl From<SourceError> for ProjectProblem {
    fn from(error: SourceError) -> Self {
        match error {
            SourceError::NotRegular => Self::NotAFile,
            SourceError::TooLarge => Self::TooLarge,
            SourceError::Unreadable => Self::NotUtf8,
            SourceError::Changing => Self::Changing,
            _ => Self::Unavailable,
        }
    }
}

/// Why a key that is written in the file may not be the intended reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Doubt {
    /// The entry and the library disagree on DOI, title or year.
    Fields,
    /// A key differs only by letter case; BibTeX treats them as one.
    LetterCase,
    /// The key is written as another entry's `ids` alias.
    Alias,
    /// The key belongs to an `@xdata` entry, which cannot be cited.
    Helper,
    /// The key is inside `@comment`. Biber skips it; classic BibTeX reads it.
    Commented,
}

/// One emitted key against one bibliography.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum KeyCheck {
    In {
        line: usize,
    },
    /// Only after a complete scan of a coherent read.
    Missing,
    CouldNotCheck,
    Repeated {
        lines: Vec<usize>,
    },
    MayNameAnother {
        line: usize,
        doubt: Doubt,
    },
}

/// One coherent read: the keys as written, and its records only for comparing.
pub struct ProjectBibliography {
    keys: KeyScan,
    records: HashMap<String, Record>,
    digest: Fingerprint,
    coherent: bool,
}

impl std::fmt::Debug for ProjectBibliography {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectBibliography")
            .field("complete", &self.keys.complete())
            .finish_non_exhaustive()
    }
}

impl ProjectBibliography {
    fn from_text(path: &Path, text: &str) -> Result<Self, ProjectProblem> {
        let keys = scan::scan(text.trim_start_matches('\u{feff}'));
        let loaded = file::parse_for_project(path, text).map_err(ProjectProblem::from)?;
        let records = loaded
            .snapshot
            .objects()
            .iter()
            .filter_map(|object| match &object.object {
                LibraryObject::Record(record) => {
                    let key = record.citation_key().ok().flatten()?.to_owned();
                    Some((key, record.clone()))
                }
                _ => None,
            })
            .collect();
        let coherent = keys.complete() && !keys.has_repeated_keys();
        Ok(Self {
            keys,
            records,
            digest: digest(text),
            coherent,
        })
    }

    pub fn complete(&self) -> bool {
        self.keys.complete()
    }

    /// A full scan is not necessarily usable identity evidence: repeated keys
    /// have had their records removed by the loader, even with valid syntax.
    fn coherent(&self) -> bool {
        self.coherent
    }

    fn check(&self, key: &str, library: Option<&Record>) -> KeyCheck {
        let found = self.keys.occurrences(key);
        if let [_, _, ..] = found {
            return KeyCheck::Repeated {
                lines: found.iter().map(|occurrence| occurrence.line).collect(),
            };
        }
        // A damaged block could hold this key, or a second copy of it.
        if !self.keys.complete() {
            return KeyCheck::CouldNotCheck;
        }
        let alias = self.keys.alias_line(key);
        let [occurrence] = found else {
            return match alias {
                Some(line) => KeyCheck::MayNameAnother {
                    line,
                    doubt: Doubt::Alias,
                },
                None if self.keys.other_spelling(key) => KeyCheck::MayNameAnother {
                    line: self.keys.spelling_line(key).unwrap_or_default(),
                    doubt: Doubt::LetterCase,
                },
                None => match self.keys.commented_line(key) {
                    Some(line) => KeyCheck::MayNameAnother {
                        line,
                        doubt: Doubt::Commented,
                    },
                    None => KeyCheck::Missing,
                },
            };
        };
        let line = occurrence.line;
        let doubt = if occurrence.helper {
            Some(Doubt::Helper)
        } else if alias.is_some() {
            Some(Doubt::Alias)
        } else if self.keys.other_spelling(key) {
            Some(Doubt::LetterCase)
        } else if library.zip(self.records.get(key)).is_some_and(|(a, b)| disagree(a, b)) {
            Some(Doubt::Fields)
        } else {
            None
        };
        match doubt {
            Some(doubt) => KeyCheck::MayNameAnother { line, doubt },
            None => KeyCheck::In { line },
        }
    }
}

/// The newest read of the chosen file, and nothing older: a read that fails
/// checks no key, and a damaged one can call no key missing.
#[derive(Clone, Default)]
pub struct ProjectRead {
    read: Option<Arc<ProjectBibliography>>,
    problem: Option<ProjectProblem>,
    /// Native identity of the selected bibliography; never serialized or logged.
    path: Option<PathBuf>,
}

impl std::fmt::Debug for ProjectRead {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectRead")
            .field("problem", &self.problem)
            .finish_non_exhaustive()
    }
}

impl ProjectRead {
    /// Read `path`. An unchanged file is not parsed again, so opening the
    /// picker stays cheap; `previous` is used for nothing else.
    pub fn read(path: &Path, previous: Option<&Self>) -> Self {
        let previous = previous.filter(|read| read.path.as_deref() == Some(path));
        // The owner of this read waits on it, so no failure may leave it waiting.
        let read = panic_guard::attempt(|| Self::observe(path, previous)).unwrap_or(Err(ProjectProblem::Unavailable));
        let (read, problem) = match read {
            Ok(read) => (Some(read), None),
            Err(problem) => (None, Some(problem)),
        };
        Self {
            read,
            problem,
            path: Some(path.to_path_buf()),
        }
    }

    fn observe(path: &Path, previous: Option<&Self>) -> Result<Arc<ProjectBibliography>, ProjectProblem> {
        let text = observed_text(path)?;
        let seen = digest(&text);
        if let Some(same) = previous
            .and_then(|previous| previous.read.clone())
            .filter(|read| read.digest == seen)
        {
            return Ok(same);
        }
        ProjectBibliography::from_text(path, &text).map(Arc::new)
    }

    /// Whether every answer stays the same, so nobody needs to ask again.
    pub fn same_answers(&self, other: &Self) -> bool {
        let same_file = match (&self.read, &other.read) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        same_file && self.path == other.path && self.problem == other.problem
    }

    /// Lines that could not be read, so a person can find them.
    pub fn damaged_lines(&self) -> &[usize] {
        self.read.as_ref().map_or(&[], |read| &read.keys.damaged)
    }

    pub fn problem(&self) -> Option<ProjectProblem> {
        self.problem
    }

    pub fn bibliography(&self) -> Option<&ProjectBibliography> {
        self.read.as_deref()
    }

    pub fn name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "the project bibliography".into())
    }

    /// Every entry was read and no key is written twice.
    pub fn current_complete(&self) -> bool {
        self.read.as_ref().is_some_and(|read| read.coherent())
    }

    /// An entry for `key` names a different work: it and the library's record
    /// carry valid DOIs that differ. Every entry counts, a repeated key's too,
    /// and damage elsewhere in the file does not hide one.
    pub fn conflicts(&self, key: &str, library: &Record) -> bool {
        let (Some(read), Some(expected)) = (&self.read, doi(library)) else {
            return false;
        };
        let written = read.keys.dois(key).iter().filter_map(|text| parse_doi(text));
        read.records
            .get(key)
            .and_then(doi)
            .into_iter()
            .chain(written)
            .any(|doi| doi != expected)
    }

    /// `library` is the record the key came from, when it has one.
    pub fn check(&self, key: &str, library: Option<&Record>) -> KeyCheck {
        self.read
            .as_ref()
            .map_or(KeyCheck::CouldNotCheck, |read| read.check(key, library))
    }
}

fn doi(record: &Record) -> Option<String> {
    parse_doi(&field(record, "DOI")?)
}

fn parse_doi(text: &str) -> Option<String> {
    match ibis_core::identifier::parse_query(text)? {
        ibis_core::identifier::Identifier::Doi(doi) => Some(doi),
        _ => None,
    }
}

fn observed_text(path: &Path) -> Result<Arc<String>, ProjectProblem> {
    let bibtex = path.extension().is_none_or(|extension| {
        !["json", "csljson"]
            .iter()
            .any(|json| extension.eq_ignore_ascii_case(json))
    });
    if !bibtex {
        return Err(ProjectProblem::NotBibtex);
    }
    Ok(file::stable_observation(path).map_err(ProjectProblem::from)?.text())
}

fn digest(text: &str) -> Fingerprint {
    Fingerprint::of_bytes(text.as_bytes())
}

/// Disagreement needs a value on both sides; absence proves nothing. A DOI
/// names a work, so two equal DOIs settle it and title or year are not compared.
fn disagree(library: &Record, project: &Record) -> bool {
    if let (Some(a), Some(b)) = (doi(library), doi(project)) {
        return a != b;
    }
    let title = |record: &Record| field(record, "title").map(|title| plain(&title));
    let different = |a: Option<String>, b: Option<String>| a.zip(b).is_some_and(|(a, b)| a != b);
    different(title(library), title(project)) || different(year(library), year(project))
}

fn field(record: &Record, name: &str) -> Option<String> {
    record
        .csl
        .get(name)?
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Letters and digits only, so case, punctuation and markup never disagree.
fn plain(text: &str) -> String {
    strip_markup(text)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn year(record: &Record) -> Option<String> {
    let first = record.csl.get("issued")?.get("date-parts")?.get(0)?.get(0)?;
    match first {
        Value::Number(number) => Some(number.to_string()),
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        _ => None,
    }
}

/// Why an entry's original text cannot be copied, so a person can re-export it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum EntryRefusal {
    /// The reference does not come from a BibTeX file, so no original exists.
    NoOriginal,
    /// The file changed since the library was read, or could not be read.
    Changed,
    Repeated,
    /// It relies on a `crossref`, `xdata` or `@string` that cannot come with it.
    Dependency,
}

/// `key`'s original entry in `text`, with what it needs to stand on its own.
pub(crate) fn entry_in(text: &str, key: &str) -> Result<String, EntryRefusal> {
    scan::entry_text(text.trim_start_matches('\u{feff}'), key).map_err(|problem| match problem {
        scan::EntryProblem::NotFound => EntryRefusal::Changed,
        scan::EntryProblem::Repeated => EntryRefusal::Repeated,
        scan::EntryProblem::Dependency => EntryRefusal::Dependency,
    })
}

#[cfg(test)]
impl ProjectBibliography {
    fn parsed(text: &str) -> Self {
        match Self::from_text(Path::new("/invented/thesis.bib"), text) {
            Ok(bibliography) => bibliography,
            Err(problem) => panic!("{problem:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use KeyCheck::*;

    fn read(text: &str) -> ProjectRead {
        ProjectRead {
            read: Some(Arc::new(ProjectBibliography::parsed(text))),
            ..ProjectRead::default()
        }
    }
    fn record(text: &str) -> Record {
        let bibliography = ProjectBibliography::parsed(text);
        bibliography
            .records
            .into_values()
            .next()
            .unwrap_or_else(|| panic!("a record"))
    }

    const TWO: &str = "@book{taylor2026,title={Synthetic signals},year={2026}}\n\n@article{morgan2025,title={Synthetic noise},year={2025}}\n";

    #[test]
    fn a_key_is_in_or_missing_only_after_a_complete_scan() {
        let project = read(TWO);
        assert_eq!(project.check("taylor2026", None), In { line: 1 });
        assert_eq!(project.check("morgan2025", None), In { line: 3 });
        assert_eq!(project.check("okafor2018", None), Missing);
    }

    #[test]
    fn text_that_is_not_an_entry_holds_no_key() {
        let project = read(
            "taylor2026, in prose\n@comment{@book{commented2026,title={Hidden}}}\n@string{strung2026 = {A}}\n\
             @book{real2026,note={see taylor2026}}\n",
        );
        for key in ["taylor2026", "strung2026"] {
            assert_eq!(project.check(key, None), Missing, "{key}");
        }
        // Biber skips it and classic BibTeX reads it, so neither answer is safe.
        assert_eq!(
            project.check("commented2026", None),
            MayNameAnother {
                line: 2,
                doubt: Doubt::Commented
            }
        );
        assert_eq!(project.check("real2026", None), In { line: 4 });
    }

    #[test]
    fn a_repeated_key_is_named_with_every_line_even_when_the_loader_drops_it() {
        let project = read("@book{twice2026,title={A}}\n@book{twice2026,title={B}}\n");
        assert_eq!(project.check("twice2026", None), Repeated { lines: vec![1, 2] });
    }

    #[test]
    fn damage_means_could_not_check_never_missing() {
        for damaged in [
            "@book{taylor2026,title={Synthetic signals}}\n@article{broken2025,title={Never closed\n",
            "@book{taylor2026,title={Synthetic signals}}\n@article{,title={No key}}\n",
            "@string{broken = }\n@book{taylor2026,title={Synthetic signals}}\n",
        ] {
            let project = read(damaged);
            assert!(!project.damaged_lines().is_empty() || project.bibliography().is_some());
            assert!(!project.bibliography().is_some_and(ProjectBibliography::complete));
            assert_eq!(project.check("okafor2018", None), CouldNotCheck, "{damaged}");
            assert_eq!(project.check("taylor2026", None), CouldNotCheck, "{damaged}");
        }
        assert_eq!(
            read("@book{a,}\n@book{a,}\n@article{b,title={Never closed\n").check("a", None),
            Repeated { lines: vec![1, 2] },
            "a repeat stays true whatever else is damaged"
        );
    }

    #[test]
    fn an_entry_the_reader_does_not_recognise_means_nothing_can_be_called_missing() {
        for text in [
            "@my-type{hyphen2026,title={A}}\n@book{real2026,}\n",
            "@book{real2026,}\n@ article{spaced2026,title={B}}\n",
            "@book{real2026,}\n\n@typé{accent2026,title={C}}\n",
        ] {
            let project = read(text);
            assert_eq!(project.check("okafor2018", None), CouldNotCheck, "{text}");
        }
        let lines = read("@book{a,}\n\n@my-type{b,}\n");
        assert_eq!(lines.damaged_lines(), [3]);
        // Prose with an address and no entry shape stays prose.
        let prose = read("Written by me@invented.example, 2026.\n@book{a,}\n");
        assert_eq!(prose.check("okafor2018", None), Missing);
    }

    #[test]
    fn a_file_of_nothing_but_entry_shapes_stays_cheap_and_undecided() {
        let text = "@a-b{\n".repeat(2_000_000);
        let started = std::time::Instant::now();
        let scan = crate::bibtex::project::scan(&text);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "{:?}",
            started.elapsed()
        );
        assert!(!scan.complete());
        assert_eq!(
            &scan.damaged[..3],
            [1, 2, 3],
            "lines stay right however they are counted"
        );
        assert!(scan.damaged.len() <= 32);
        // Prose inside a comment can hide an entry shape too.
        assert_eq!(
            read("@comment{@my-type{k,}}\n").check("okafor2018", None),
            CouldNotCheck
        );
    }

    #[test]
    fn a_file_read_again_unchanged_keeps_its_answer_and_new_damage_is_noticed() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join("thesis.bib");
        let write = |text: &str| std::fs::write(&path, text).unwrap_or_else(|error| panic!("{error}"));
        write(TWO);
        let good = ProjectRead::read(&path, None);
        assert!(good.same_answers(&ProjectRead::read(&path, Some(&good))));

        write(&format!("{TWO}@book{{x,title={{Never closed\n"));
        let damaged = ProjectRead::read(&path, Some(&good));
        assert_eq!(damaged.problem(), None);
        assert_eq!(damaged.damaged_lines(), [4]);
        assert_eq!(damaged.check("taylor2026", None), CouldNotCheck);
        assert!(
            damaged.same_answers(&ProjectRead::read(&path, Some(&damaged))),
            "the same damage again"
        );

        write(&format!("{TWO}\n@book{{x,title={{Never closed\n"));
        let moved = ProjectRead::read(&path, Some(&damaged));
        assert!(
            !moved.same_answers(&damaged),
            "other lines are shown, so the picker must hear of it"
        );

        write(TWO);
        let recovered = ProjectRead::read(&path, Some(&moved));
        assert_eq!(recovered.problem(), None);
        assert!(recovered.damaged_lines().is_empty());
    }

    #[test]
    fn a_key_with_no_comma_after_it_is_not_an_entry() {
        let project = read("@book{taylor2026 title={Synthetic signals}}\n");
        assert_eq!(project.check("taylor2026", None), CouldNotCheck);
        assert_eq!(
            read("@book{ taylor2026 , title={A}}\n").check("taylor2026", None),
            In { line: 1 }
        );
        assert_eq!(read("@book{taylor2026}\n").check("taylor2026", None), In { line: 1 });
    }

    #[test]
    fn line_endings_and_parentheses_read_as_any_other_entry() {
        let project = read("@book(paren2026,\r\n  title = {A}\r\n)\r\n\r\n@BOOK{upper2026,}\r\n");
        assert_eq!(project.check("paren2026", None), In { line: 1 });
        assert_eq!(project.check("upper2026", None), In { line: 5 });
        assert_eq!(project.check("okafor2018", None), Missing);
    }

    #[test]
    fn a_key_that_is_there_in_another_form_raises_a_doubt() {
        let project = read(
            "@book{Taylor2026,ids={oldkey2020},title={A}}\n@xdata{shared,publisher={B}}\n@book{Case2026,}\n@book{case2026,}\n",
        );
        let doubt = |key, line, doubt| {
            assert_eq!(project.check(key, None), MayNameAnother { line, doubt }, "{key}");
        };
        doubt("taylor2026", 1, Doubt::LetterCase);
        doubt("oldkey2020", 1, Doubt::Alias);
        doubt("shared", 2, Doubt::Helper);
        doubt("Case2026", 3, Doubt::LetterCase);
        assert_eq!(project.check("Taylor2026", None), In { line: 1 });
    }

    #[test]
    fn an_alias_list_built_from_a_macro_could_name_anything() {
        let project = read("@string{m={x}}\n@book{a,ids=m}\n");
        assert_eq!(project.check("okafor2018", None), CouldNotCheck);
    }

    #[test]
    fn differing_fields_raise_a_doubt_and_formatting_does_not() {
        let project = read(TWO);
        let check = |library: &str| project.check("taylor2026", Some(&record(library)));
        assert_eq!(
            check("@book{taylor2026,title={Synthetic {S}ignals.},year={2026}}"),
            In { line: 1 }
        );
        // Absence proves nothing.
        assert_eq!(check("@book{taylor2026,author={Taylor, Avery}}"), In { line: 1 });
        for other in [
            "@book{taylor2026,title={Another book},year={2026}}",
            "@book{taylor2026,title={Synthetic signals},year={1999}}",
        ] {
            assert_eq!(
                check(other),
                MayNameAnother {
                    line: 1,
                    doubt: Doubt::Fields
                },
                "{other}"
            );
        }
        let with_doi = read("@book{a,title={One},doi={10.1000/ONE}}\n");
        let same = record("@book{a,title={Quite another title},doi={10.1000/one}}");
        assert_eq!(with_doi.check("a", Some(&same)), In { line: 1 });
        let other = record("@book{a,title={One},doi={10.1000/two}}");
        assert!(matches!(with_doi.check("a", Some(&other)), MayNameAnother { .. }));
    }

    #[test]
    fn files_are_read_only_when_they_can_be_trusted() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let problem = |path: &Path| ProjectRead::read(path, None).problem();
        let absent = dir.path().join("absent.bib");
        assert_eq!(problem(&absent), Some(ProjectProblem::Unavailable));
        assert_eq!(problem(dir.path()), Some(ProjectProblem::NotAFile));
        let binary = dir.path().join("binary.bib");
        std::fs::write(&binary, [0xff, 0xfe, 0x00]).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(problem(&binary), Some(ProjectProblem::NotUtf8));
        let json = dir.path().join("refs.json");
        std::fs::write(&json, "[]").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(problem(&json), Some(ProjectProblem::NotBibtex));
        let good = dir.path().join("thesis.bib");
        std::fs::write(&good, format!("\u{feff}{TWO}")).unwrap_or_else(|error| panic!("{error}"));
        let project = ProjectRead::read(&good, None);
        assert_eq!(project.problem(), None);
        assert_eq!(project.check("taylor2026", None), In { line: 1 });
    }

    #[test]
    fn a_missing_entry_comes_with_what_it_needs_and_stays_as_written() {
        let text = "@string{press = {Imaginary} # \" \" # city}\n\n@string{city = {Press}}\n\n@string{unused = {x}}\n\n\
                    @string{inner = {Inner}}\n@string{city2 = inner # {Two}}\n\n\
                    @inbook{child2026, crossref = {parent2026}, xdata={shared},\n  title = {Chapter}, month=jan, note=city2 }\n\n\
                    @book{parent2026,publisher=city}\n\n@xdata{shared,year=2026}\n\n@book{other2026,}\n";
        let entry = entry_in(text, "child2026").unwrap_or_else(|problem| panic!("{problem:?}"));
        assert_eq!(
            entry,
            "@string{city = {Press}}\n\n@string{inner = {Inner}}\n\n@string{city2 = inner # {Two}}\n\n\
             @inbook{child2026, crossref = {parent2026}, xdata={shared},\n  title = {Chapter}, month=jan, note=city2 }\n\n\
             @book{parent2026,publisher=city}\n\n@xdata{shared,year=2026}\n"
        );
        assert!(!entry.contains("unused") && !entry.contains("other2026"));
    }

    #[test]
    fn an_entry_that_cannot_stand_alone_is_refused() {
        for (text, refusal) in [
            ("@book{a,crossref={gone}}\n", EntryRefusal::Dependency),
            ("@book{a,publisher=gone}\n", EntryRefusal::Dependency),
            (
                "@book{a,publisher=later}\n@string{later={x}}\n",
                EntryRefusal::Dependency,
            ),
            (
                "@string{m={x}}\n@book{a,publisher=m}\n@string{m={y}}\n",
                EntryRefusal::Dependency,
            ),
            ("@book{a,crossref=m}\n@string{m={x}}\n", EntryRefusal::Dependency),
            ("@book{a,xref={gone}}\n", EntryRefusal::Dependency),
            ("@set{a,entryset={gone}}\n", EntryRefusal::Dependency),
            (
                "@string{broken = }\n@string{m={x}}\n@book{a,publisher=m}\n",
                EntryRefusal::Dependency,
            ),
            (
                "@book{a,crossref={b}}\n@book{b,title={Never closed\n",
                EntryRefusal::Dependency,
            ),
            ("@book{a,}\n@book{a,title={Never closed\n", EntryRefusal::Repeated),
            ("@book{a,}\n@book{a,}\n", EntryRefusal::Repeated),
            ("@book{b,}\n", EntryRefusal::Changed),
            ("@xdata{a,}\n", EntryRefusal::Changed),
        ] {
            assert_eq!(entry_in(text, "a"), Err(refusal), "{text}");
        }
    }

    #[test]
    fn a_set_comes_with_its_members_and_a_related_entry_with_its_target() {
        let text = "@set{a,entryset={b,c}}\n@book{b,}\n@book{c,related={d}}\n@book{d,}\n@book{e,}\n";
        let entry = entry_in(text, "a").unwrap_or_else(|problem| panic!("{problem:?}"));
        for needed in ["@set{a,", "@book{b,", "@book{c,", "@book{d,"] {
            assert!(entry.contains(needed), "{entry}");
        }
        assert!(!entry.contains("@book{e,"), "{entry}");
    }

    #[test]
    fn a_crossref_cycle_ends() {
        let text = "@book{a,crossref={b}}\n@book{b,crossref={a}}\n";
        assert_eq!(
            entry_in(text, "a").as_deref(),
            Ok("@book{a,crossref={b}}\n\n@book{b,crossref={a}}\n")
        );
    }
}
