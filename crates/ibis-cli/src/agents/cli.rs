use super::{Client, Context, Error, Result, readiness};
use crate::{Failure, OutputMode};
use clap::{Arg, ArgAction, ArgMatches, Command};
use ibis_core::contract::ErrorCode;
use ibis_engine::DispatchResult;
use serde_json::json;
use std::io::{BufRead, Write};

pub(crate) fn command() -> Command {
    let client = || {
        Arg::new("client")
            .long("client")
            .value_name("NAME")
            .action(ArgAction::Append)
            .value_parser(clap::builder::PossibleValuesParser::new(Client::ALL.map(Client::name)))
            .help(format!(
                "Which AI app to connect or remove; repeat for several: {}",
                Client::ALL.map(Client::name).join(", ")
            ))
    };
    Command::new("agents")
        .about("Connect Ibis to AI apps on this computer")
        .subcommand_required(true)
        .subcommand(
            Command::new("setup")
                .about("Connect an AI app to Ibis and its citation skill")
                .arg(client())
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .action(ArgAction::SetTrue)
                        .help("Show the planned changes without writing files or opening apps"),
                )
                .arg(
                    Arg::new("yes").long("yes").action(ArgAction::SetTrue).help(
                        "Connect selected apps, or all found apps, without asking; required when not at a terminal",
                    ),
                ),
        )
        .subcommand(Command::new("status").about("Show AI app connections and library readiness"))
        .subcommand(
            Command::new("remove")
                .about("Remove only the connections and skill files Ibis added")
                .arg(client()),
        )
}

fn result(data: serde_json::Value, text: String) -> DispatchResult {
    DispatchResult {
        data,
        text: Some(text),
        source_findings: false,
        exit_code: ibis_core::contract::ExitCode::Ok,
    }
}

pub(crate) fn run(
    args: &ArgMatches,
    input: &mut impl BufRead,
    errors: &mut impl Write,
    mode: OutputMode,
) -> std::result::Result<DispatchResult, Failure> {
    execute(args, input, errors, mode).map_err(|e| Failure::new(ErrorCode::BadInput, e.to_string()))
}

fn execute(
    args: &ArgMatches,
    input: &mut impl BufRead,
    errors: &mut impl Write,
    mode: OutputMode,
) -> Result<DispatchResult> {
    let (name, args) = args
        .subcommand()
        .ok_or_else(|| Error("choose setup, status or remove".into()))?;
    if name == "setup" && mode != OutputMode::Terminal && !args.get_flag("yes") {
        return Err(Error(
            "setup needs a terminal to ask; use --yes when not interactive (and --client NAME to select apps)".into(),
        ));
    }
    let context = Context::from_environment()?;
    if name == "status" {
        let clients = context.status()?;
        let ready = readiness();
        let mut text = format!("Ibis {}: {}\n", ready.version, ready.health);
        for library in &ready.libraries {
            text.push_str(&format!(
                "{}: {} items, {}\n",
                library.display_name, library.item_count, library.health
            ));
        }
        for client in &clients {
            text.push_str(&format!("{}: {}", client.client.name(), client.message));
            if let Some(command) = &client.command {
                text.push_str(&format!(" ({})", command.display()));
            }
            text.push('\n');
        }
        text.push_str("ChatGPT on the web cannot run local Ibis tools.\n");
        let mut data = serde_json::to_value(&ready).map_err(|_| Error("cannot describe Ibis readiness".into()))?;
        data["clients"] =
            serde_json::to_value(clients).map_err(|_| Error("cannot describe AI app connections".into()))?;
        return Ok(result(data, text.trim_end().into()));
    }
    let mut clients = args
        .get_many::<String>("client")
        .into_iter()
        .flatten()
        .filter_map(|name| Client::ALL.into_iter().find(|c| c.name() == name))
        .collect::<Vec<_>>();
    if clients.is_empty() {
        if name == "remove" {
            clients = Client::ALL.to_vec();
        } else {
            if mode != OutputMode::Terminal && !args.get_flag("yes") {
                return Err(Error(
                    "setup without --client needs a terminal; use --yes to connect all found apps".into(),
                ));
            }
            clients = Client::ALL.into_iter().filter(|c| context.found(*c)).collect();
            if mode == OutputMode::Terminal && !args.get_flag("yes") && !clients.is_empty() {
                for (index, client) in clients.iter().enumerate() {
                    writeln!(errors, "{}. {}", index + 1, client.name())
                        .map_err(|_| Error("cannot list AI apps".into()))?;
                }
                write!(
                    errors,
                    "Connect which apps? Enter numbers separated by spaces, 'all', or Enter to cancel: "
                )
                .and_then(|_| errors.flush())
                .map_err(|_| Error("cannot ask which apps to connect".into()))?;
                let mut answer = String::new();
                input
                    .read_line(&mut answer)
                    .map_err(|_| Error("could not read the selection; no settings were changed".into()))?;
                if answer.trim() != "all" {
                    clients = answer
                        .split_whitespace()
                        .map(|number| {
                            number
                                .parse::<usize>()
                                .ok()
                                .and_then(|n| n.checked_sub(1))
                                .and_then(|n| clients.get(n))
                                .copied()
                                .ok_or_else(|| {
                                    Error("choose one of the listed app numbers; no settings were changed".into())
                                })
                        })
                        .collect::<Result<Vec<_>>>()?;
                }
            }
        }
    }
    let plan = if name == "remove" {
        context.remove(&clients)?
    } else {
        context.setup(&clients)?
    };
    let dry = name == "setup" && args.get_flag("dry-run");
    let mut text = String::new();
    for action in plan.actions() {
        text.push_str(&format!(
            "{} {} {}\n",
            if dry { "Would" } else { "Will" },
            action.operation,
            action.path.display()
        ));
    }
    if plan.actions().is_empty() {
        text.push_str(if clients.is_empty() {
            "No AI apps selected or found.\n"
        } else {
            "No changes needed.\n"
        });
    }
    for message in plan.messages() {
        text.push_str(message);
        text.push('\n');
    }
    let confirmations = plan.confirmations().to_vec();
    let data = json!({"dry_run":dry,"actions":plan.actions(),"messages":plan.messages()});
    if !dry {
        plan.apply()?;
        for bundle in confirmations {
            super::open_confirmation(&bundle)?;
        }
        text = text.replace("Will ", "Did ");
    }
    Ok(result(data, text.trim_end().into()))
}
