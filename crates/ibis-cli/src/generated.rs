//! Files made from the command line itself; `node scripts/gen.mjs` writes them.
use clap::{Arg, Command};
use std::fmt::Write as _;

/// Every command's options as a Markdown section for `docs/cli.md`.
fn reference(root: &Command) -> String {
    let mut out =
        String::from("### Every option\n\nThe options each command takes, as `ibis COMMAND --help` lists them.\n");
    let global: Vec<&Arg> = root.get_arguments().filter(|arg| arg.is_global_set()).collect();
    out.push_str("\n**Every command** also takes:\n\n");
    table(&mut out, global.iter().copied());
    for command in root.get_subcommands().filter(|command| !command.is_hide_set()) {
        section(&mut out, &format!("ibis {}", command.get_name()), command);
        for nested in command.get_subcommands().filter(|nested| !nested.is_hide_set()) {
            section(
                &mut out,
                &format!("ibis {} {}", command.get_name(), nested.get_name()),
                nested,
            );
        }
    }
    out
}

fn section(out: &mut String, name: &str, command: &Command) {
    let about = command.get_about().map(ToString::to_string).unwrap_or_default();
    let _ = write!(out, "\n**`{name}`**: {about}\n");
    // Library choices and the global options are listed once, above.
    let own: Vec<&Arg> = command
        .get_arguments()
        .filter(|arg| !arg.is_hide_set() && !arg.is_global_set() && arg.get_help_heading() != Some(super::LIBRARIES))
        .filter(|arg| !matches!(arg.get_id().as_str(), "help" | "version"))
        .collect();
    if !own.is_empty() {
        out.push('\n');
        table(out, own.into_iter());
    }
}

fn table<'a>(out: &mut String, args: impl Iterator<Item = &'a Arg>) {
    out.push_str("| Option | Does |\n| --- | --- |\n");
    for arg in args {
        let help = arg.get_help().map(ToString::to_string).unwrap_or_default();
        let _ = writeln!(out, "| `{}` | {} |", spelling(arg), help.replace('|', "\\|"));
    }
}

/// How the option is typed: `--limit N`, or `WORDS…` for a positional.
fn spelling(arg: &Arg) -> String {
    let values = arg
        .get_value_names()
        .map(|names| names.iter().map(ToString::to_string).collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| arg.get_id().as_str().to_uppercase());
    let takes_value = arg.get_num_args().is_some_and(|range| range.takes_values());
    let many = arg.get_num_args().is_some_and(|range| range.max_values() > 1);
    let values = if many { format!("{values}…") } else { values };
    match (arg.get_long(), arg.get_short()) {
        (Some(long), _) if takes_value => format!("--{long} {values}"),
        (Some(long), _) => format!("--{long}"),
        (None, Some(short)) if takes_value => format!("-{short} {values}"),
        (None, Some(short)) => format!("-{short}"),
        (None, None) => values,
    }
}

#[test]
#[ignore = "writes the generated files for gen.mjs"]
fn write_generated() {
    let directory = std::env::var_os("IBIS_GEN_DIR").unwrap_or_else(|| panic!("set IBIS_GEN_DIR"));
    let directory = std::path::Path::new(&directory);
    let mut schema = Vec::new();
    let status = super::run_with(
        ibis_engine::Engine::from_snapshot(std::sync::Arc::new(
            ibis_core::contract::Snapshot::new(vec![]).unwrap_or_else(|e| panic!("{e}")),
        )),
        ["ibis", "schema", "--json"].map(Into::into).to_vec(),
        std::io::empty(),
        &mut schema,
        std::io::sink(),
        super::OutputMode::Plain,
    );
    assert_eq!(status, 0);
    let schema: serde_json::Value = serde_json::from_slice(&schema).unwrap_or_else(|e| panic!("{e}"));
    let schema = serde_json::to_string_pretty(&schema).unwrap_or_else(|e| panic!("{e}")) + "\n";
    std::fs::write(directory.join("cli-schema.json"), schema).unwrap_or_else(|e| panic!("{e}"));
    // Building fills in what each option takes, as `--help` shows it.
    let mut root = super::command();
    root.build();
    std::fs::write(directory.join("cli-options.md"), reference(&root)).unwrap_or_else(|e| panic!("{e}"));
}
