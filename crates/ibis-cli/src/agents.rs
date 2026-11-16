//! Command-line questions and reports for the engine's shared AI app connections.
use crate::Failure;
use clap::{Arg, ArgAction, ArgMatches, Command};
use ibis_core::contract::{ErrorCode, ExitCode};
use ibis_engine::{
    DispatchResult,
    agent_apps::{self, ARGS, AgentApp, AgentAppState, COMMAND, Change, GUIDE, Home, Outcome},
};
use serde_json::json;
use std::io::{BufRead, Write};
use std::path::PathBuf;

pub(crate) fn command() -> Command {
    let clients = Arg::new("client")
        .long("client")
        .value_name("APP")
        .action(ArgAction::Append)
        .value_parser(agent_apps::ids().collect::<Vec<_>>())
        .help("Only this app; repeat for several");
    let dry_run = Arg::new("dry-run")
        .long("dry-run")
        .action(ArgAction::SetTrue)
        .help("Show what would change and change nothing");
    let yes = Arg::new("yes")
        .long("yes")
        .short('y')
        .action(ArgAction::SetTrue)
        .help("Do it without asking; needed when no one can answer, such as in a script");
    Command::new("agents")
        .about("Connect Ibis to the AI apps on this computer, show them, or disconnect them")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("setup")
                .about("Add Ibis's read-only MCP server to each AI app found here")
                .after_help(format!(
                    "Other servers and settings are kept. A different ibis entry is shown and replaced only if you agree (or with --replace).\nUndo with: ibis agents remove\nBy hand: {GUIDE}"
                ))
                .arg(clients.clone())
                .arg(dry_run.clone())
                .arg(yes.clone())
                .arg(
                    Arg::new("replace")
                        .long("replace")
                        .action(ArgAction::SetTrue)
                        .help("Replace a different ibis entry without asking"),
                ),
        )
        .subcommand(Command::new("status").about("Show which AI apps have Ibis connected").arg(clients.clone()))
        .subcommand(
            Command::new("remove")
                .about("Take Ibis's server out of each AI app; other servers are kept")
                .arg(clients)
                .arg(dry_run)
                .arg(yes),
        )
}

fn home_here() -> Option<Home> {
    let root = std::env::home_dir()?;
    let config = if cfg!(target_os = "macos") {
        Some(root.join("Library").join("Application Support"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| Some(root.join(".config")))
    };
    let mut home = Home {
        config,
        path: std::env::var_os("PATH")
            .map(|path| std::env::split_paths(&path).collect())
            .unwrap_or_default(),
        codex_home: std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute()),
        ibis: None,
        root,
    };
    home.ibis = home.find(COMMAND).or_else(|| std::env::current_exe().ok());
    Some(home)
}

struct Line {
    app: AgentApp,
    change: Change,
}

pub(crate) fn run<R: BufRead, E: Write>(
    matches: &ArgMatches,
    input: &mut R,
    errors: &mut E,
    interactive: bool,
) -> Result<DispatchResult, Failure> {
    let home = home_here().ok_or_else(|| Failure::new(ErrorCode::SourceUnavailable, "no home folder is known here"))?;
    run_in(&home, matches, input, errors, interactive)
}

fn run_in<R: BufRead, E: Write>(
    home: &Home,
    matches: &ArgMatches,
    input: &mut R,
    errors: &mut E,
    interactive: bool,
) -> Result<DispatchResult, Failure> {
    let (action, args) = matches.subcommand().unwrap_or(("status", matches));
    let chosen: Vec<&str> = args
        .get_many::<String>("client")
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect();
    let apps = agent_apps::plan(home, &chosen);
    let flag = |name: &str| args.try_get_one::<bool>(name).ok().flatten().copied().unwrap_or(false);
    let (dry_run, yes) = (flag("dry-run"), flag("yes"));
    let mut lines = vec![];
    match action {
        "status" => {
            for app in apps {
                let change = app.status();
                lines.push(Line { app, change });
            }
        }
        "setup" | "remove" => {
            let setup = action == "setup";
            let changes = apps.iter().filter(|app| app.would_change(setup)).count();
            let go = dry_run
                || changes == 0
                || yes
                || (interactive
                    && ask(
                        input,
                        errors,
                        &format!(
                            "{} Ibis {} {changes} app{}. Continue?",
                            if setup { "Connect" } else { "Disconnect" },
                            if setup { "to" } else { "from" },
                            if changes == 1 { "" } else { "s" }
                        ),
                    ));
            if !go && !interactive {
                return Err(Failure::new(
                    ErrorCode::Usage,
                    "nothing was changed. Add --yes to change other apps' settings without being asked, or --dry-run to see the plan.",
                ));
            }
            for app in apps {
                let mut replace = flag("replace");
                if setup && go && !dry_run && !replace && interactive && app.state == AgentAppState::Different {
                    replace = ask(
                        input,
                        errors,
                        &format!(
                            "{} has a different ibis entry:\n{}\nReplace it?",
                            app.name,
                            app.diff.as_deref().unwrap_or_default()
                        ),
                    );
                }
                let change = if setup {
                    app.connect(home, go && !dry_run, dry_run, replace)
                } else {
                    app.disconnect(home, go && !dry_run, dry_run)
                };
                lines.push(Line { app, change });
            }
        }
        _ => return Err(Failure::new(ErrorCode::Usage, "unknown agents command")),
    }
    Ok(report(action, &lines, home, dry_run))
}

fn ask<R: BufRead, E: Write>(input: &mut R, errors: &mut E, question: &str) -> bool {
    let _ = write!(errors, "{question} [y/N] ");
    let _ = errors.flush();
    let mut answer = String::new();
    input.read_line(&mut answer).is_ok() && matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn report(action: &str, lines: &[Line], home: &Home, dry_run: bool) -> DispatchResult {
    let mut text = String::new();
    let on_path = home.on_path(COMMAND);
    if lines.is_empty() {
        text.push_str(&format!(
            "No supported AI app was found here. Set one up by hand: {GUIDE}\n"
        ));
    }
    for line in lines {
        let detail = if line.change.detail.is_empty() {
            String::new()
        } else {
            format!(": {}", line.change.detail)
        };
        text.push_str(&format!(
            "{:<16} {}{detail}\n",
            line.app.name,
            line.change.outcome.as_str().replace('-', " ")
        ));
    }
    if !on_path && action != "remove" {
        text.push_str("\nThe ibis command isn't on PATH, so apps can't start it. Install it first: ");
        text.push_str(GUIDE);
        text.push('\n');
    }
    match action {
        "setup" | "remove" if dry_run => text.push_str("\nNothing was changed (--dry-run).\n"),
        "setup"
            if lines
                .iter()
                .any(|line| matches!(line.change.outcome, Outcome::Connected | Outcome::Replaced)) =>
        {
            text.push_str("\nRestart each app, or start a new session, to use Ibis. To undo: ibis agents remove\n");
        }
        _ => {}
    }
    let problem = lines.iter().any(|line| {
        matches!(
            line.change.outcome,
            Outcome::Failed | Outcome::Blocked | Outcome::Different | Outcome::Stale | Outcome::Kept
        )
    });
    DispatchResult {
        data: json!({
            "action": action,
            "dry_run": dry_run,
            "ibis_on_path": on_path,
            "server": {"command": COMMAND, "args": ARGS},
            "apps": lines.iter().map(|line| json!({
                "id": line.app.id, "name": line.app.name, "outcome": line.change.outcome, "detail": line.change.detail,
            })).collect::<Vec<_>>(),
        }),
        text: Some(text.trim_end().to_owned()),
        source_findings: false,
        exit_code: if problem { ExitCode::Findings } else { ExitCode::Ok },
    }
}

#[cfg(test)]
#[path = "agents_tests.rs"]
mod tests;
