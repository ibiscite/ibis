//! `ibis format`: the engine makes the copy (`Engine::format_file`); this
//! names the files and says what happened.
use crate::{Failure, human, human::cli};
use clap::{Arg, ArgMatches, Command};
use ibis_core::contract::{CitationError, ErrorCode, ExitCode};
use ibis_engine::office::{Part, Refusal};
use ibis_engine::{
    Cancellation, DispatchResult, Engine, FormatIssue, FormatOutcome, FormatPlace, FormatRefusal, FormatRequest, Issue,
    KeyProblem,
};
use ibis_core::syntax::marker::MarkerProblemKind;
use serde_json::json;
use std::io::Write;
use std::path::PathBuf;

pub(crate) fn command() -> Command {
    Command::new("format")
        .about("Make a finished copy of a Word or LibreOffice document from its citation markers")
        .after_help(
            "Write citations as the markers Ibis pastes as Pandoc keys, such as [@key], [@key, p. 12],\n\
             [see @key; -@other] or @key, and a paragraph reading [bibliography] where the reference\n\
             list goes; without one it ends the document. The original is never changed and nothing\n\
             is replaced. Run it again on the original with another --style to restyle.\n\n\
             Examples:\n  \
             ibis format draft.docx --style apa -o final.docx --file refs.bib\n  \
             ibis format draft.odt --style ieee -o final.odt --zotero",
        )
        .arg(
            Arg::new("input")
                .value_name("DOCUMENT")
                .required(true)
                .value_parser(clap::value_parser!(PathBuf))
                .help("The .docx or .odt document with citation markers"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("PATH")
                .required(true)
                .value_parser(clap::value_parser!(PathBuf))
                .help("The new copy to write, with the same extension; nothing there is replaced"),
        )
        .arg(
            Arg::new("style")
                .long("style")
                .value_name("STYLE_ID")
                .default_value("apa")
                .help("Any bundled in-text CSL style; list them with: ibis styles"),
        )
        .arg(
            Arg::new("locale")
                .long("locale")
                .value_name("LOCALE")
                .help("Language of the citations, such as de-DE; default the style's own"),
        )
}

pub(crate) fn run(engine: &Engine, args: &ArgMatches, errors: &mut impl Write) -> Result<DispatchResult, Failure> {
    let path = |id: &str| args.get_one::<PathBuf>(id).cloned().unwrap_or_default();
    let (input, output) = (path("input"), path("output"));
    let style = args.get_one::<String>("style").cloned().unwrap_or_default();
    let request = FormatRequest {
        input: input.clone(),
        output: output.clone(),
        style: style.clone(),
        output_locale: args.get_one::<String>("locale").cloned(),
    };
    let named = |path: &PathBuf| human::field(&path.display().to_string());
    let outcome = engine
        .format_file(&request, &Cancellation::default())
        .map_err(|error| match error.reason {
            Some(ibis_core::contract::ErrorReason::UnknownStyle) => {
                Failure::explained(error, cli("cli-style-unknown", &[("style", &human::field(&style))]))
            }
            Some(ibis_core::contract::ErrorReason::UnknownLocale) => Failure::explained(
                error,
                cli(
                    "cli-locale-unknown",
                    &[("locale", &human::field(args.get_one::<String>("locale").map_or("", String::as_str)))],
                ),
            ),
            _ => Failure::from(error),
        })?;
    let report = match outcome {
        FormatOutcome::Written(report) => report,
        FormatOutcome::Refused(refusal) => return Err(refused(&refusal, &named(&input), &named(&output), &style)),
    };
    if !report.left.is_empty() {
        let mut note = format!("ibis: {}\n", cli("cli-format-left", &[]));
        for place in &report.left {
            note.push_str(&format!("  {}\n", place_text(place)));
        }
        let _ = errors.write_all(note.as_bytes());
    }
    let plural = |count: usize| if count == 1 { "one" } else { "other" };
    let text = cli(
        "cli-format-written",
        &[
            ("path", &named(&output)),
            ("citations", &report.citations.to_string()),
            ("citations-plural", plural(report.citations)),
            ("references", &report.references.to_string()),
            ("references-plural", plural(report.references)),
            ("style", &human::field(&style)),
        ],
    );
    Ok(DispatchResult {
        data: json!({ "written": report }),
        text: Some(text),
        source_findings: false,
        exit_code: ExitCode::Ok,
    })
}

fn refused(refusal: &FormatRefusal, input: &str, output: &str, style: &str) -> Failure {
    let (code, message) = match refusal {
        FormatRefusal::Markers(issues) => {
            let code = if issues
                .iter()
                .all(|issue| matches!(issue.issue, Issue::Key { problem: KeyProblem::Missing, .. }))
            {
                ErrorCode::NotFound
            } else {
                ErrorCode::BadInput
            };
            let mut message = cli("cli-format-markers", &[("path", output)]);
            for issue in issues {
                message.push_str(&format!("\n  {}: {}", place_text(&issue.place), issue_text(issue, style)));
            }
            (code, message)
        }
        FormatRefusal::Document(refusal) => {
            let (code, id) = match refusal {
                Refusal::Missing => (ErrorCode::NotFound, "cli-format-missing"),
                Refusal::NotLocal => (ErrorCode::BadInput, "cli-format-not-local"),
                Refusal::NotOffice => (ErrorCode::BadInput, "cli-format-not-office"),
                Refusal::Unsupported => (ErrorCode::BadInput, "cli-format-unsupported"),
                Refusal::Damaged => (ErrorCode::BadInput, "cli-format-damaged"),
                Refusal::Unsafe => (ErrorCode::BadInput, "cli-format-unsafe"),
                Refusal::TooLarge => (ErrorCode::ResourceLimit, "cli-format-too-large"),
                Refusal::TimedOut => (ErrorCode::ResourceLimit, "cli-format-timed-out"),
                Refusal::Unavailable | Refusal::Mismatch => (ErrorCode::Internal, "cli-format-unavailable"),
            };
            (code, cli(id, &[("path", input)]))
        }
        FormatRefusal::NoMarkers => (ErrorCode::BadInput, cli("cli-format-no-markers", &[("path", input)])),
        FormatRefusal::NoteStyle => (ErrorCode::BadInput, cli("cli-format-note-style", &[("style", &human::field(style))])),
        FormatRefusal::StyleCannotWrite => (
            ErrorCode::BadInput,
            cli("cli-format-style-cannot", &[("style", &human::field(style))]),
        ),
        FormatRefusal::OutputExists => (ErrorCode::BadInput, cli("cli-format-exists", &[("path", output)])),
        FormatRefusal::SameFile => (ErrorCode::Usage, cli("cli-format-same-file", &[])),
        FormatRefusal::OutputKind => (ErrorCode::Usage, cli("cli-format-kind", &[])),
        FormatRefusal::OutputUnwritable => (ErrorCode::BadInput, cli("cli-format-unwritable", &[("path", output)])),
    };
    Failure::new(code, message)
}

fn place_text(place: &FormatPlace) -> String {
    let part = match place.part {
        Part::Body => "body",
        Part::Note => "note",
        Part::Header => "header",
        Part::Comment => "comment",
        Part::TextBox => "box",
        Part::Hidden => "hidden",
    };
    cli(
        "cli-format-place",
        &[
            ("part", part),
            ("paragraph", &place.paragraph.to_string()),
            ("column", &place.column.to_string()),
            ("text", &human::field(&place.text)),
        ],
    )
}

fn issue_text(issue: &FormatIssue, style: &str) -> String {
    match &issue.issue {
        Issue::Marker { problem } => cli(
            match problem {
                MarkerProblemKind::Unclosed => "cli-format-unclosed",
                MarkerProblemKind::WithoutKey => "cli-format-without-key",
                MarkerProblemKind::TwoKeys => "cli-format-two-keys",
                MarkerProblemKind::Locator => "cli-format-locator",
                MarkerProblemKind::TooLong => "cli-format-too-long",
            },
            &[],
        ),
        Issue::SecondBibliography => cli("cli-format-second-bibliography", &[]),
        Issue::Key { key, problem } => {
            let key = human::field(key);
            let id = match problem {
                KeyProblem::Missing => "cli-format-key-missing",
                KeyProblem::Ambiguous => "cli-format-key-ambiguous",
                KeyProblem::Duplicate => "cli-format-key-duplicate",
                KeyProblem::Damaged => "cli-format-key-damaged",
                KeyProblem::CutShort => "cli-format-key-cut-short",
                KeyProblem::Invalid => "cli-format-key-invalid",
                KeyProblem::LocatorNotPrinted => "cli-format-key-locator",
                KeyProblem::Citation(error) => return citation_text(*error, &key),
            };
            cli(id, &[("key", &key), ("style", &human::field(style))])
        }
    }
}

fn citation_text(error: CitationError, key: &str) -> String {
    let reason = ibis_engine::catalogue::text(error.message_id(), None, &[]).unwrap_or_default();
    format!("{key}: {reason}")
}
