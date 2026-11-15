//! `ibis proposals list|show|reject`: read and reject the library changes AI
//! apps proposed through `ibis mcp --allow-writes` (decision 0055). There is
//! no apply here: the person applies a proposal in the Ibis app, after its
//! preview, so no agent with a shell can confirm its own proposal.

use clap::{Arg, ArgAction, ArgMatches, Command};
use ibis_core::contract::{ErrorCode, ExitCode};
use ibis_engine::DispatchResult;
use ibis_engine::write::{Preview, Proposal, Proposals, Status, WriteError, WriteProblem};
use serde_json::json;

use crate::{Failure, settings_backup};

pub(crate) fn command() -> Command {
    let id = || {
        Arg::new("id")
            .value_name("ID")
            .required(true)
            .help("The proposal id printed by ibis proposals list")
    };
    Command::new("proposals")
        .about("List, show or reject library changes AI apps proposed; apply them in the Ibis app")
        .after_help("AI apps propose changes only when `ibis mcp` runs with --allow-writes. Nothing in a library changes until you apply a proposal in the Ibis app.")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(
            Arg::new("nightly")
                .long("nightly")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Use Ibis Nightly's proposals"),
        )
        .subcommand(Command::new("list").about("List proposals, newest first"))
        .subcommand(
            Command::new("show")
                .about("Show what a proposal would change, with the entry before and after")
                .arg(id()),
        )
        .subcommand(
            Command::new("reject")
                .about("Reject a pending proposal; no library changes")
                .arg(id()),
        )
}

pub(crate) fn run(matches: &ArgMatches) -> Result<DispatchResult, Failure> {
    let (_, settings) = settings_backup::settings_file(matches.get_flag("nightly"))?;
    let store = Proposals::beside(&settings);
    let id = |args: &ArgMatches| args.get_one::<String>("id").cloned().unwrap_or_default();
    match matches.subcommand() {
        Some(("list", _)) => {
            let proposals = store.list().map_err(failure)?;
            let text = if proposals.is_empty() {
                "No proposals.\n".to_owned()
            } else {
                proposals.iter().map(line).collect()
            };
            Ok(result(json!({"proposals": proposals}), text))
        }
        Some(("show", args)) => {
            let preview = store.show(&id(args)).map_err(failure)?;
            let text = shown(&preview);
            Ok(result(json!(preview), text))
        }
        Some(("reject", args)) => {
            let proposal = store.reject(&id(args)).map_err(failure)?;
            let text = format!("Rejected {}; no library changed.\n", proposal.id);
            Ok(result(json!(proposal), text))
        }
        _ => Err(Failure::new(ErrorCode::Usage, "name an action: list, show or reject")),
    }
}

fn failure(error: WriteError) -> Failure {
    let code = match error.problem {
        WriteProblem::UnknownProposal => ErrorCode::NotFound,
        WriteProblem::Decided => ErrorCode::BadInput,
        _ => ErrorCode::SourceUnavailable,
    };
    Failure::new(code, error.sentence())
}

fn status(status: Status) -> &'static str {
    match status {
        Status::Pending => "pending",
        Status::Rejected => "rejected",
        Status::Applied => "applied",
    }
}

fn line(proposal: &Proposal) -> String {
    format!(
        "{}  {:<8}  {}  {}\n",
        proposal.id,
        status(proposal.status),
        proposal.target.name(),
        ibis_engine::write::summary(proposal)
    )
}

fn shown(preview: &Preview) -> String {
    let proposal = &preview.proposal;
    let mut text = format!(
        "{} ({}), from {} through MCP, for {}\n{}\n",
        proposal.id,
        status(proposal.status),
        proposal.source.client,
        preview.library,
        preview.summary
    );
    for line in &preview.lines {
        text.push_str(&format!("  {line}\n"));
    }
    if let Some(before) = &preview.before {
        text.push_str(&format!("\nNow:\n{before}\n"));
    }
    if let Some(after) = &preview.after {
        text.push_str(&format!("\nAfter applying:\n{after}\n"));
    }
    match preview.refusal {
        Some(WriteProblem::Decided) => {}
        Some(problem) => text.push_str(&format!(
            "\nThis cannot be applied now: {}\n",
            WriteError { problem, field: None }.sentence()
        )),
        None => text.push_str("\nApply or reject it in the Ibis app.\n"),
    }
    text
}

/// The printer ends the text with its own newline.
fn result(data: serde_json::Value, text: String) -> DispatchResult {
    DispatchResult {
        data,
        text: Some(text.trim_end().to_owned()),
        source_findings: false,
        exit_code: ExitCode::Ok,
    }
}
