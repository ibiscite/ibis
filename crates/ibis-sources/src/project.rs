//! The bibliography a project compiles against, chosen by the person and read
//! only to check keys. It is never a search library: loading it as one would
//! give every key a second owner, and could not prove a key absent.
//!
//! Presence is all a check can say. It does not show that the fields match,
//! that the export is current or that the document compiles.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use ibis_core::contract::{LibraryObject, Record};
use ibis_core::model::strip_markup;
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
        Ok(Self { keys, records })
    }

    pub fn complete(&self) -> bool {
        self.keys.complete()
    }

    /// Lines that could not be read, so a person can find them.
    pub fn damaged_lines(&self) -> &[usize] {
        &self.keys.damaged
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
                None => KeyCheck::Missing,
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

/// The newest read of the chosen file. A failed read keeps the last good copy
/// and says so: it can still show a key, but can no longer prove one missing.
#[derive(Debug, Clone, Default)]
pub struct ProjectRead {
    good: Option<Arc<ProjectBibliography>>,
    problem: Option<ProjectProblem>,
}

impl ProjectRead {
    /// Read `path`, keeping `previous`'s good copy if this read fails.
    pub fn read(path: &Path, previous: Option<&Self>) -> Self {
        Self::after(previous, read_project(path))
    }

    fn after(previous: Option<&Self>, read: Result<ProjectBibliography, ProjectProblem>) -> Self {
        match read {
            Ok(bibliography) => Self {
                good: Some(Arc::new(bibliography)),
                problem: None,
            },
            Err(problem) => Self {
                good: previous.and_then(|previous| previous.good.clone()),
                problem: Some(problem),
            },
        }
    }

    pub fn problem(&self) -> Option<ProjectProblem> {
        self.problem
    }

    /// The answers come from an earlier read, because this one failed.
    pub fn last_good(&self) -> bool {
        self.problem.is_some() && self.good.is_some()
    }

    pub fn bibliography(&self) -> Option<&ProjectBibliography> {
        self.good.as_deref()
    }

    /// `library` is the record the key came from, when it has one.
    pub fn check(&self, key: &str, library: Option<&Record>) -> KeyCheck {
        let Some(good) = &self.good else {
            return KeyCheck::CouldNotCheck;
        };
        match good.check(key, library) {
            KeyCheck::Missing if self.problem.is_some() => KeyCheck::CouldNotCheck,
            check => check,
        }
    }
}

fn read_project(path: &Path) -> Result<ProjectBibliography, ProjectProblem> {
    let bibtex = path.extension().is_none_or(|extension| {
        !["json", "csljson"]
            .iter()
            .any(|json| extension.eq_ignore_ascii_case(json))
    });
    if !bibtex {
        return Err(ProjectProblem::NotBibtex);
    }
    let observation = file::stable_observation(path).map_err(ProjectProblem::from)?;
    ProjectBibliography::from_text(path, observation.text())
}

/// Disagreement needs a value on both sides; absence proves nothing. A DOI
/// names a work, so two equal DOIs settle it and title or year are not compared.
fn disagree(library: &Record, project: &Record) -> bool {
    let doi = |record: &Record| field(record, "DOI").map(|doi| doi.to_lowercase());
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
        ProjectRead::after(None, Ok(ProjectBibliography::parsed(text)))
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
        for key in ["taylor2026", "commented2026", "strung2026"] {
            assert_eq!(project.check(key, None), Missing, "{key}");
        }
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
    fn a_failed_read_keeps_the_last_good_copy_labelled_and_proves_nothing_missing() {
        let good = read(TWO);
        let failed = ProjectRead::after(Some(&good), Err(ProjectProblem::Unavailable));
        assert!(failed.last_good());
        assert_eq!(failed.problem(), Some(ProjectProblem::Unavailable));
        assert_eq!(failed.check("taylor2026", None), In { line: 1 });
        assert_eq!(failed.check("okafor2018", None), CouldNotCheck);

        // A later good read clears the label; a first failure has nothing to keep.
        let recovered = ProjectRead::after(Some(&failed), Ok(ProjectBibliography::parsed(TWO)));
        assert!(!recovered.last_good());
        assert_eq!(recovered.check("okafor2018", None), Missing);
        let first = ProjectRead::after(None, Err(ProjectProblem::Changing));
        assert!(!first.last_good());
        assert_eq!(first.check("taylor2026", None), CouldNotCheck);
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
            ("@book{a,}\n@book{a,}\n", EntryRefusal::Repeated),
            ("@book{b,}\n", EntryRefusal::Changed),
            ("@xdata{a,}\n", EntryRefusal::Changed),
        ] {
            assert_eq!(entry_in(text, "a"), Err(refusal), "{text}");
        }
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
