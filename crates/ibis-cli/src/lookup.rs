//! `ibis lookup DOI`: cite a DOI from the open libraries, or, while online
//! lookup is on, from the record a DOI service gives (decisions 0008 and 0051).
//! It is not an engine method, so `call`, `serve` and MCP cannot reach it.

use crate::{
    DispatchResult, Execution, Failure, OutputMode, Response, chosen_form, chosen_output, dispatch, form_args, human,
    human::cli, locale_arg, out::Out, output_args, output_name, settings_backup, write_stream,
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use ibis_core::contract::{CitationItem, CitationRequest, EngineError, ErrorCode, Origin};
use ibis_engine::lookup::{Disclosure, Found, Lookup};
use ibis_engine::outputs::DEFAULT_STYLE;
use ibis_engine::{Cancellation, Engine, Method, Reply};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::{BufRead, Write};

pub(crate) fn command() -> Command {
    Command::new("lookup")
        .about("Cite a DOI: from your libraries, or looked up online if you turned that on")
        .after_help(
            "A DOI your libraries hold is cited from them, and nothing is sent. Otherwise, only while online \
             lookup is on, the DOI alone is sent to doi.org, which hands it to Crossref or DataCite; Ibis asks \
             once, in a terminal, before the first lookup. The record is in none of your libraries.\n\n\
             Examples:\n  \
             ibis lookup 10.5555/12345678 --style apa\n  \
             ibis lookup https://doi.org/10.5555/12345678 --style apa --bibliography",
        )
        .arg(
            Arg::new("doi")
                .value_name("DOI")
                .required(true)
                .help("A DOI, a doi.org link or doi:10.…"),
        )
        .args(output_args(DEFAULT_STYLE))
        .args(form_args())
        .arg(locale_arg())
        .arg(
            Arg::new("strict")
                .long("strict")
                .action(ArgAction::SetTrue)
                .help(ibis_engine::catalogue::text("cli-strict-help", None, &[]).unwrap_or_default()),
        )
        .arg(
            Arg::new("nightly")
                .long("nightly")
                .action(ArgAction::SetTrue)
                .help("Use Ibis Nightly's library choice and lookup settings"),
        )
}

/// Who the record is from, beside the citation, in `--json`.
#[derive(Debug, serde::Serialize)]
pub(crate) struct Looked {
    pub origin: Origin,
    pub citation: Reply,
}

pub(crate) fn run<R: BufRead, E: Write>(
    engine: &Engine,
    args: &ArgMatches,
    names: &BTreeMap<String, String>,
    input: &mut R,
    errors: &mut E,
    mode: OutputMode,
) -> Result<Execution, Failure> {
    let cancel = Cancellation::default();
    let query = args.get_one::<String>("doi").cloned().unwrap_or_default();
    let output = chosen_output(args, DEFAULT_STYLE)?;
    let form = chosen_form(args, &output)?;
    let settings = || {
        settings_backup::settings_file(args.get_flag("nightly"))
            .map(|(_, path)| ibis_engine::settings::Owner::open(path))
            .map_err(|failure| failure.error)
    };
    let interactive = mode.stdin_terminal() && mode.stderr_terminal();
    let mut ask = |disclosure: &Disclosure| {
        if !interactive {
            return None;
        }
        confirm(disclosure, input, errors)
    };
    let found = Lookup::new().doi(engine, settings, &query, &mut ask, &cancel);
    let locale = args.get_one::<String>("locale");
    let strict = args.get_flag("strict");
    let (origin, result, label, note) = match crate::explain(found, |error| {
        ibis_engine::catalogue::error(&error, ibis_engine::catalogue::Audience::Cli)
    })? {
        Found::InLibrary(found) if found.hits.len() > 1 => {
            let result = human::with_summary(
                found.into_dispatch().map_data(Response::Engine).bounded()?,
                args.get_flag("json"),
                human::Holdings::References,
                names,
                Out::configured(mode, args),
            );
            return Ok(Execution::Search(result, vec![cli("cli-lookup-several", &[])]));
        }
        Found::InLibrary(found) => {
            let hit = found
                .hits
                .into_iter()
                .next()
                .ok_or(EngineError::new(ErrorCode::Internal))?;
            let item = hit.item.object.id().clone();
            let request = CitationRequest {
                strict,
                items: vec![CitationItem {
                    item: item.clone(),
                    expected_cite: Some(hit.item.fingerprints.cite.clone()),
                    form: None,
                    locator: None,
                    prefix: Default::default(),
                    suffix: Default::default(),
                    suppress_author: false,
                    annotation: None,
                    expected_annotation_cite: None,
                }],
                form,
                keep_order: false,
                output: output.clone(),
                input_language: None,
                output_locale: locale.cloned(),
            };
            let params = serde_json::to_value(&request).map_err(|_| EngineError::new(ErrorCode::Internal))?;
            let result = dispatch(engine, Method::Cite, &params, &cancel)?;
            (
                Origin::Library { item },
                result,
                query.clone(),
                cli("cli-lookup-in-library", &[]),
            )
        }
        Found::LookedUp(found) => {
            let params = json!({
                "records": [found.record],
                "strict": strict,
                "output": output,
                "form": form,
                "output_locale": locale,
            });
            let result = dispatch(engine, Method::Records, &params, &cancel)?;
            let service = match &found.origin {
                Origin::Lookup { service, .. } => service.host(),
                Origin::Library { .. } => "",
            };
            let note = cli("cli-lookup-looked-up", &[("service", service)]);
            (found.origin, result, query.clone(), note)
        }
    };
    let result: DispatchResult = result.map_data(|data| match data {
        Response::Engine(citation) => Response::Lookup(Box::new(Looked { origin, citation })),
        other => other,
    });
    let citing = human::Citing {
        labels: vec![label],
        flags: vec![],
        output: output_name(&output),
        form,
    };
    Ok(Execution::Lookup(result, citing, note))
}

fn confirm(disclosure: &Disclosure, input: &mut impl BufRead, errors: &mut impl Write) -> Option<bool> {
    let [first, second] = disclosure.then.map(|service| service.host());
    let question = cli(
        "cli-lookup-ask",
        &[
            ("doi", &disclosure.doi),
            ("to", disclosure.to),
            ("first", first),
            ("second", second),
        ],
    );
    let prompt = format!("{} ", crate::terminal_display(&question));
    write_stream(errors, prompt.as_bytes()).ok()?;
    let mut answer = String::new();
    input.read_line(&mut answer).ok()?;
    Some(matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::contract::LookupService;
    use std::io::{self, Cursor};

    struct Unflushed;

    impl Write for Unflushed {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("prompt did not reach the terminal"))
        }
    }

    #[test]
    fn a_yes_is_ignored_when_the_prompt_cannot_be_shown() {
        let disclosure = Disclosure {
            doi: "10.5555/invented".into(),
            to: ibis_engine::lookup::RESOLVER,
            then: LookupService::ALL,
        };
        let mut input = Cursor::new(b"yes\n");
        assert_eq!(confirm(&disclosure, &mut input, &mut Unflushed), None);
        assert_eq!(input.position(), 0);
    }
}
