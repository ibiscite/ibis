//! Files made from the command line itself; `node scripts/gen.mjs` writes them.
use clap::{Arg, Command};
use serde_json::{Map, Value, json};
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

/// The built parser, including options left out of the help reference.
fn command_surface(command: &Command, inherited: &Map<String, Value>) -> Value {
    let mut globals = Map::new();
    let arguments = command
        .get_arguments()
        .filter_map(|arg| {
            let id = arg.get_id().to_string();
            let details = argument_surface(command, arg);
            if arg.is_global_set() {
                globals.insert(id.clone(), details.clone());
                // Clap copies global arguments into descendants. Record each
                // definition where it first applies or is overridden.
                if inherited.get(&id) == Some(&details) {
                    return None;
                }
            }
            Some((id, details))
        })
        .collect::<Map<_, _>>();
    let subcommands = command
        .get_subcommands()
        .map(|command| (command.get_name().to_owned(), command_surface(command, &globals)))
        .collect::<Map<_, _>>();
    let groups = command
        .get_groups()
        .map(|group| {
            let mut group = group.clone();
            (
                group.get_id().to_string(),
                json!({
                    "arguments": group.get_args().map(ToString::to_string).collect::<Vec<_>>(),
                    "required": group.is_required_set(),
                    "multiple": group.is_multiple(),
                }),
            )
        })
        .collect::<Map<_, _>>();
    json!({
        "description": command.get_about().map(ToString::to_string),
        "aliases": command.get_all_aliases().collect::<Vec<_>>(),
        "shortFlag": command.get_short_flag(),
        "longFlag": command.get_long_flag(),
        "shortFlagAliases": command.get_all_short_flag_aliases().collect::<Vec<_>>(),
        "longFlagAliases": command.get_all_long_flag_aliases().collect::<Vec<_>>(),
        "hidden": command.is_hide_set(),
        "subcommandRequired": command.is_subcommand_required_set(),
        "argumentsRequiredElseHelp": command.is_arg_required_else_help_set(),
        "arguments": arguments,
        "groups": groups,
        "subcommands": subcommands,
    })
}

fn argument_surface(command: &Command, arg: &Arg) -> Value {
    let arity = arg.get_num_args().map(|range| {
        json!({ "min": range.min_values(), "max": (range.max_values() != usize::MAX).then_some(range.max_values()) })
    });
    let mut conflicts: Vec<_> = command
        .get_arg_conflicts_with(arg)
        .iter()
        .map(|other| other.get_id().to_string())
        .collect();
    conflicts.sort();
    conflicts.dedup();
    let values = arg
        .get_possible_values()
        .into_iter()
        .map(|value| {
            (
                value.get_name().to_owned(),
                json!({
                    "aliases": value.get_name_and_aliases().skip(1).collect::<Vec<_>>(),
                    "help": value.get_help().map(ToString::to_string),
                    "hidden": value.is_hide_set(),
                }),
            )
        })
        .collect::<Map<_, _>>();
    json!({
        "spelling": spelling(arg),
        "help": arg.get_help().map(ToString::to_string),
        "long": arg.get_long(),
        "short": arg.get_short(),
        "longAliases": arg.get_all_aliases().unwrap_or_default(),
        "shortAliases": arg.get_all_short_aliases().unwrap_or_default(),
        "valueNames": arg.get_value_names().unwrap_or_default().iter().map(ToString::to_string).collect::<Vec<_>>(),
        "index": arg.get_index(),
        "global": arg.is_global_set(),
        "hidden": arg.is_hide_set(),
        "required": arg.is_required_set(),
        "action": format!("{:?}", arg.get_action()),
        "arity": arity,
        "default": arg.get_default_values().iter().map(|value| value.to_string_lossy()).collect::<Vec<_>>(),
        "possibleValues": values,
        "delimiter": arg.get_value_delimiter(),
        "terminator": arg.get_value_terminator().map(ToString::to_string),
        "conflicts": conflicts,
        "requireEquals": arg.is_require_equals_set(),
        "exclusive": arg.is_exclusive_set(),
        "allowHyphenValues": arg.is_allow_hyphen_values_set(),
        "allowNegativeNumbers": arg.is_allow_negative_numbers_set(),
        "ignoreCase": arg.is_ignore_case_set(),
        "trailingVarArg": arg.is_trailing_var_arg_set(),
        "last": arg.is_last_set(),
    })
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
    std::fs::write(
        directory.join("picker-schema.json"),
        ibis_engine::PickerMethod::schemas().to_string(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let picker_words: std::collections::BTreeMap<_, _> = ibis_engine::catalogue::Language::ALL
        .into_iter()
        .map(|language| (language.tag(), ibis_engine::picker::shell_words(language)))
        .collect();
    std::fs::write(
        directory.join("picker-words.json"),
        serde_json::to_vec(&picker_words).unwrap_or_else(|e| panic!("{e}")),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    // Export the runtime's code-to-exit mapping rather than parsing Rust in gen.mjs.
    let errors: Vec<_> = ibis_core::contract::ErrorCode::ALL
        .into_iter()
        .map(|code| serde_json::json!({ "code": code, "exit": code.exit_code().value() }))
        .collect();
    // Building fills in what each option takes, as `--help` shows it.
    let mut root = super::command();
    root.build();
    let surface = json!({ "command": command_surface(&root, &Map::new()), "errors": errors });
    let surface = serde_json::to_string_pretty(&surface).unwrap_or_else(|e| panic!("{e}")) + "\n";
    std::fs::write(directory.join("cli-surface.json"), surface).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(directory.join("cli-options.md"), reference(&root)).unwrap_or_else(|e| panic!("{e}"));
}
