use super::*;
use crate::Cancellation;

const LIBRARY: &str = "\
@book{vale2024, author = {Vale, Avery}, title = {Lanterns of the North}, year = {2024}, publisher = {Imaginary Press}}
@article{fen2023, author = {Fen, Rowan}, title = {Clouds over invented hills}, journal = {Journal of Made-up Weather}, year = {2023}, volume = {4}, pages = {1--9}}
@book{twice, title = {One}, year = {2020}}
@book{twice, title = {Two}, year = {2021}}
";

struct Fixture {
    engine: Engine,
    folder: tempfile::TempDir,
}

impl Fixture {
    fn new(library: &str) -> Self {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().canonicalize().unwrap().join("refs.bib");
        std::fs::write(&path, library).unwrap();
        let engine = Engine::new()
            .unwrap()
            .without_file_watches()
            .with_documents_in_process();
        engine.open_file(&path, &Cancellation::default()).unwrap();
        Self { engine, folder }
    }
    fn file(&self, name: &str, text: &str) -> PathBuf {
        // macOS reaches its temporary folder through a link; documents are read without following one.
        let path = self.folder.path().canonicalize().unwrap().join(name);
        std::fs::write(&path, text).unwrap();
        path
    }
    fn check(&self, document: &Path, set: Option<&Path>) -> CheckResult {
        self.engine
            .check_document(
                &CheckRequest {
                    document: document.to_owned(),
                    set: set.map(Path::to_owned),
                },
                &Cancellation::default(),
            )
            .unwrap()
            .value
    }
    fn export(&self, document: &Path, syntax: KeySyntax) -> ExportResult {
        self.engine
            .export_document(
                &ExportRequest {
                    document: document.to_owned(),
                    output: OutputSpec::Key { syntax, command: None },
                },
                &Cancellation::default(),
            )
            .unwrap()
            .value
    }
}

fn read(result: CheckResult) -> Read {
    match result {
        CheckResult::Checked(checked) => checked.read,
        refused => panic!("{refused:?}"),
    }
}

fn problems(read: &Read) -> Vec<(usize, Option<&str>, Problem)> {
    read.findings
        .iter()
        .map(|finding| {
            (
                finding.place.map_or(0, |place| place.line),
                finding.key.as_deref(),
                finding.problem,
            )
        })
        .collect()
}

#[test]
fn a_check_names_every_citation_that_cannot_be_built() {
    let fixture = Fixture::new(LIBRARY);
    let draft = fixture.file(
        "draft.md",
        "Lanterns [@vale2024, p. 12].\n\nClouds [@fen2023; @ghost] and @twice.\n\nBroken [@vale2024\n",
    );
    let read = read(fixture.check(&draft, None));
    assert_eq!((read.kind, read.citations, read.keys), (TextKind::Markdown, 3, 4));
    assert_eq!(
        problems(&read),
        [
            (3, Some("ghost"), Problem::Missing),
            (3, Some("twice"), Problem::Duplicate),
            (5, None, Problem::Unclosed),
        ]
    );
    assert!(read.complete && !read.settled());

    let clean = fixture.file("clean.tex", "\\textcite{vale2024} and \\parencite[p.~2]{fen2023}.\n");
    let read = super::tests::read(fixture.check(&clean, None));
    assert!(read.settled(), "{read:?}");
}

#[test]
fn a_document_ibis_cannot_read_refuses_without_checking() {
    let fixture = Fixture::new(LIBRARY);
    let missing = fixture.folder.path().canonicalize().unwrap().join("gone.md");
    assert_eq!(
        fixture.check(&missing, None),
        CheckResult::Refused {
            file: Which::Document,
            reason: Refusal::Missing
        }
    );
    let docx = fixture.file("draft.docx", "PK");
    assert_eq!(
        fixture.check(&docx, None),
        CheckResult::Refused {
            file: Which::Document,
            reason: Refusal::Unsupported
        }
    );
}

#[test]
fn an_exported_set_locks_the_cited_records() {
    let fixture = Fixture::new(LIBRARY);
    let draft = fixture.file("draft.typ", "As @vale2024 says, #cite(<fen2023>) and @ghost.\n");
    let ExportResult::Exported {
        read,
        records: Some(records),
    } = fixture.export(&draft, KeySyntax::CslJson)
    else {
        panic!()
    };
    assert_eq!(problems(&read), [(1, Some("ghost"), Problem::Missing)]);
    let exported: Vec<Value> = serde_json::from_str(&records.text).unwrap();
    let keys: Vec<&str> = exported
        .iter()
        .map(|record| record["citation-key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["vale2024", "fen2023"]);
    let set = fixture.file("references.json", &records.text);

    // The set reads back as the same records: nothing has drifted.
    let read = super::tests::read(fixture.check(&draft, Some(&set)));
    assert_eq!(problems(&read), [(1, Some("ghost"), Problem::Missing)]);

    // A record that changed in the library, and a key the set never had.
    let changed = Fixture::new(&LIBRARY.replace("Lanterns of the North", "Lanterns of the Far North"));
    let draft = changed.file("draft.typ", "@vale2024 and @fen2023, then @twice.\n");
    let set = changed.file("references.json", &records.text);
    let read = super::tests::read(changed.check(&draft, Some(&set)));
    assert_eq!(
        problems(&read),
        [
            (1, Some("twice"), Problem::Duplicate),
            (1, Some("vale2024"), Problem::Changed),
        ]
    );
    let draft = changed.file("more.typ", "@vale2024 @fen2023 @fen2023\n");
    let set = changed.file("partial.json", "[]");
    let read = super::tests::read(changed.check(&draft, Some(&set)));
    assert_eq!(
        problems(&read),
        [
            (1, Some("vale2024"), Problem::NotInSet),
            (1, Some("fen2023"), Problem::NotInSet),
        ]
    );
    let damaged = changed.file("damaged.json", "{\"not\": \"a set\"}");
    assert_eq!(
        changed.check(&draft, Some(&damaged)),
        CheckResult::Refused {
            file: Which::Set,
            reason: Refusal::Damaged
        }
    );
}

#[test]
fn export_writes_bibtex_and_refuses_citation_outputs() {
    let fixture = Fixture::new(LIBRARY);
    let draft = fixture.file("draft.md", "[@fen2023; @vale2024] and [@fen2023].\n");
    let ExportResult::Exported {
        records: Some(records), ..
    } = fixture.export(&draft, KeySyntax::Bibtex)
    else {
        panic!()
    };
    assert!(records.text.starts_with("@article{fen2023,"), "{}", records.text);
    assert_eq!(records.text.matches("@book{vale2024,").count(), 1);
    let failure = fixture
        .engine
        .export_document(
            &ExportRequest {
                document: draft,
                output: OutputSpec::Csl { style: "apa".into() },
            },
            &Cancellation::default(),
        )
        .unwrap_err();
    assert_eq!(failure, EngineError::citation(CitationError::UnsupportedOutput));
}

#[test]
fn a_large_set_reads_back_and_a_wildcard_is_named_on_export() {
    let title = "Lanterns ".repeat(6_000);
    let library: String = (0..12)
        .map(|n| format!("@book{{vale{n}, author = {{Vale, Avery}}, title = {{{title}{n}}}, year = {{2024}}}}\n"))
        .collect();
    let fixture = Fixture::new(&library);
    let cites: String = (0..12).map(|n| format!("\\cite{{vale{n}}} ")).collect();
    let draft = fixture.file("draft.tex", &format!("{cites}\\nocite{{*}}\n"));
    let ExportResult::Exported {
        read,
        records: Some(records),
    } = fixture.export(&draft, KeySyntax::CslJson)
    else {
        panic!()
    };
    assert!(records.text.len() > 512 * 1024, "{} {:?}", records.text.len(), read);
    assert_eq!(problems(&read), [(0, None, Problem::CitesEverything)]);
    let set = fixture.file("references.json", &records.text);
    let read = super::tests::read(fixture.check(&draft, Some(&set)));
    assert!(read.settled(), "{read:?}");
}
