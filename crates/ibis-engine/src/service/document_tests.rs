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
                    quotes: false,
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

#[cfg(unix)]
#[test]
fn formatting_refuses_linked_parents_before_comparing_file_identity() {
    use super::format::{FormatRefusal, FormatRequest, FormatResult};
    let fixture = Fixture::new(LIBRARY);
    let root = fixture.folder.path().canonicalize().unwrap();
    let draft = fixture.file("draft.docx", "not parsed during preflight");
    let linked = root.join("linked");
    std::os::unix::fs::symlink(&root, &linked).unwrap();
    for (document, output, expected) in [
        (
            linked.join("draft.docx"),
            draft.clone(),
            FormatRefusal::Document(Refusal::NotLocal),
        ),
        (
            draft.clone(),
            linked.join("draft.docx"),
            FormatRefusal::OutputUnwritable,
        ),
        (draft.clone(), linked.join("new.docx"), FormatRefusal::OutputUnwritable),
    ] {
        assert_eq!(
            fixture
                .engine
                .format_document(
                    &FormatRequest {
                        document,
                        output,
                        style: "apa".into(),
                        output_locale: None
                    },
                    &Cancellation::default(),
                )
                .unwrap(),
            FormatResult::Refused(expected)
        );
    }
    assert_eq!(std::fs::read_to_string(&draft).unwrap(), "not parsed during preflight");
    assert!(!root.join("new.docx").exists());
}

#[test]
fn formatting_detects_the_same_file_by_checked_handle_identity() {
    use super::format::{FormatRefusal, FormatRequest, FormatResult};
    let fixture = Fixture::new(LIBRARY);
    let draft = fixture.file("draft.docx", "not parsed during preflight");
    let alias = draft.with_file_name("alias.docx");
    std::fs::hard_link(&draft, &alias).unwrap();
    assert_eq!(
        fixture
            .engine
            .format_document(
                &FormatRequest {
                    document: draft.clone(),
                    output: alias,
                    style: "apa".into(),
                    output_locale: None
                },
                &Cancellation::default(),
            )
            .unwrap(),
        FormatResult::Refused(FormatRefusal::SameFile)
    );
    assert_eq!(std::fs::read_to_string(&draft).unwrap(), "not parsed during preflight");
}

#[test]
fn a_cancelled_format_stops_before_file_identity_preflight() {
    use super::format::FormatRequest;
    let fixture = Fixture::new(LIBRARY);
    let draft = fixture.file("draft.docx", "not parsed during preflight");
    let cancel = Cancellation::default();
    cancel.cancel();
    let result = fixture.engine.format_document(
        &FormatRequest {
            document: draft.clone(),
            output: draft,
            style: "apa".into(),
            output_locale: None,
        },
        &cancel,
    );
    assert_eq!(result.unwrap_err().code, ErrorCode::Cancelled);
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
    // Both co-authors' versions of a record survived a merge.
    let mut merged: Vec<Value> = serde_json::from_str(&records.text).unwrap();
    merged.push(
        serde_json::json!({"id": "vale2024-theirs", "citation-key": "vale2024", "type": "book", "title": "Theirs"}),
    );
    let set = changed.file("merged.json", &serde_json::to_string(&merged).unwrap());
    let read = super::tests::read(changed.check(&draft, Some(&set)));
    assert_eq!(problems(&read)[0], (1, Some("vale2024"), Problem::TwiceInSet));
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

#[cfg(unix)]
#[test]
fn a_quotation_is_checked_against_the_cited_page_of_the_source() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new(
        "@book{vale2024, title = {Lanterns of the North}, year = {2024}, file = {vale.pdf}}\n\
         @book{fen2023, title = {Clouds over invented hills}, year = {2023}}\n",
    );
    // A stand-in for `ibis __extract-pdf`: two pages printed as 11 and 12.
    let program = fixture.folder.path().join("invented-ibis");
    std::fs::write(
        &program,
        "#!/bin/sh\ncat >/dev/null\nprintf '{\"read\":{\"pages\":2,\"labels\":[\"11\",\"12\"],\"text\":[\"The lanterns guide\\\\nevery ship home.\",\"Tides turn when the moon is low.\"]}}'\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(
        fixture.folder.path().canonicalize().unwrap().join("vale.pdf"),
        "%PDF-1.7\n",
    )
    .unwrap();
    fixture.engine.reload_files(None, &Cancellation::default()).unwrap();
    let engine = fixture.engine.clone().with_pdf_reader(program);
    let draft = fixture.file(
        "draft.md",
        "\u{201c}The lanterns guide every ship\u{201d} [@vale2024, p. 11].\n\n\
         \"Tides turn\" [@vale2024, p. 11].\n\n\
         \"The lanterns \u{2026} home\" [@vale2024, pp. 10-12].\n\n\
         \"Lanterns never fail\" [@vale2024].\n\n\
         \"Clouds gather\" [@fen2023].\n",
    );
    let request = CheckRequest {
        document: draft,
        set: None,
        quotes: true,
    };
    let read = read(engine.check_document(&request, &Cancellation::default()).unwrap().value);
    let found: Vec<_> = read
        .findings
        .iter()
        .map(|finding| {
            (
                finding.place.map(|place| place.line),
                finding.problem,
                finding.pages.clone(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            (Some(3), Problem::QuoteOnOtherPage, vec!["12".to_owned()]),
            (Some(7), Problem::QuoteNotFound, vec![]),
            (Some(9), Problem::QuoteUnchecked, vec![]),
        ]
    );
}

#[test]
fn a_cited_page_holds_its_following_pages_lettered_parts_and_roman_ranges() {
    for (locator, page) in [
        ("12", "12"),
        ("12-14", "13"),
        ("12–14", "14"),
        ("2f", "3"),
        ("2f.", "2"),
        ("2ff", "40"),
        ("12a", "12"),
        ("4, 9", "9"),
        ("i-iii", "ii"),
        ("IV", "iv"),
        ("xii f", "xiii"),
        ("A-3", "A-3"),
    ] {
        assert!(on_page(locator, page), "{locator} holds {page}");
    }
    for (locator, page) in [
        ("12", "13"),
        ("2f", "4"),
        ("12-14", "15"),
        ("ii", "2"),
        ("2", "ii"),
        ("i-iii", "iv"),
        ("iiii", "iv"),
        ("12ab", "12"),
    ] {
        assert!(!on_page(locator, page), "{locator} does not hold {page}");
    }
}
