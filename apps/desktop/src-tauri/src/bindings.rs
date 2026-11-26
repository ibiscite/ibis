//! One command registry drives both the runtime handler and the generated
//! bindings: `node scripts/gen.mjs` turns `contract()` into `src/bindings.ts`.
//! Only the handler ships; the rest exists in tests, which write the contract.
#[cfg(test)]
use schemars::{JsonSchema, Schema, SchemaGenerator, generate::SchemaSettings};
#[cfg(test)]
use serde_json::{Value, json};

/// One command as the web view calls it.
#[cfg(test)]
pub(crate) struct Command {
    pub(crate) name: &'static str,
    pub(crate) docs: &'static str,
    /// Each argument by its name in the web view.
    pub(crate) args: Vec<(&'static str, Schema)>,
    /// What a successful call returns; `None` when the command returns nothing.
    pub(crate) returns: Option<Schema>,
    /// What a failed call throws, for a command that returns a `Result`.
    pub(crate) error: Option<Schema>,
}

/// What the web view sends is read as Rust deserializes it, and what it
/// receives as Rust serializes it; a type can differ between the two.
#[cfg(test)]
pub(crate) struct Schemas {
    input: SchemaGenerator,
    output: SchemaGenerator,
}

#[cfg(test)]
impl Schemas {
    pub(crate) fn input<T: ?Sized + JsonSchema>(&mut self) -> Schema {
        self.input.subschema_for::<T>()
    }
    pub(crate) fn output<T: ?Sized + JsonSchema>(&mut self) -> Schema {
        self.output.subschema_for::<T>()
    }
}

macro_rules! registry {
    ($($command:tt)*) => {
        /// The handler for every command the web view may call.
        pub(crate) fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
            tauri::generate_handler![$($command)*]
        }
        #[cfg(test)]
        fn commands() -> Vec<fn(&mut Schemas) -> Command> {
            ibis_desktop_macros::commands![$($command)*]
        }
    };
}

registry![
    crate::app_context::app_context,
    crate::proposals::waiting_proposals,
    crate::proposals::apply_proposal,
    crate::proposals::reject_proposal,
    crate::proposals::reject_app_proposals,
    crate::agent_apps::agent_apps,
    crate::agent_apps::connect_agent_app,
    crate::agent_apps::disconnect_agent_app,
    crate::library::attachment_text::attachment_text_libraries,
    crate::library::attachment_text::set_attachment_text,
    crate::library::attachment_text::attachment_text_outside_library,
    crate::library::attachment_text::set_attachment_text_outside_library,
    crate::cli_install::cli_status,
    crate::cli_install::set_cli_installed,
    crate::cli_install::copy_cli_profile,
    crate::tray::copy_version,
    crate::library::commands::choose_library,
    crate::library::commands::reload_library,
    crate::library::commands::use_zotero,
    crate::library::commands::use_sample_library,
    crate::setup::find_zotero,
    crate::setup::find_sources,
    crate::setup::open_found_library,
    crate::library::commands::use_zotero_libraries,
    crate::library::commands::zotero_groups,
    crate::library::commands::zotero_profiles,
    crate::library::commands::use_zotero_profile,
    crate::library::commands::check_zotero_sharing,
    crate::library::commands::library_status,
    crate::library::open_source::open_library_source,
    crate::library::open_source::library_source,
    crate::library::commands::search,
    crate::library::commands::recent_references,
    crate::copy::preview_file_citation,
    crate::copy::citation_details,
    crate::copy::citation_example,
    crate::delivery::deliver_citation,
    crate::project::choose_project,
    crate::project::copy_project_entry,
    crate::editor_endpoint::editor_request,
    crate::editor_endpoint::editor_endpoint,
    crate::editor_endpoint::set_editor_endpoint,
    crate::editor_endpoint::copy_editor_endpoint,
    crate::session_references::session_references,
    crate::session_references::edit_session_references,
    crate::session_references::session_bibliography,
    crate::session_references::copy_session_bibliography,
    crate::session_references::export_session_bibliography,
    crate::preferences::picker_preferences,
    crate::preferences::save_picker_preferences,
    crate::delights::delight_view,
    crate::delights::shorthand_hint,
    crate::preferences::picker_preferences_saved,
    crate::destination::remember_app_output,
    crate::destination::forget_app_output,
    crate::format_settings::format_settings,
    crate::format_settings::forget_app_format,
    crate::settings_window::open_settings_file,
    crate::login_item::login_item,
    crate::login_item::set_login_item,
    crate::picker::snapshot::picker_snapshot,
    crate::picker::snapshot::picker_rendered,
    crate::picker::hide_picker,
    crate::picker::set_pinned,
    crate::picker::picker_moving,
    crate::picker::quit,
    crate::picker::shortcut_status,
    crate::picker::wayland::configure_shortcuts,
    crate::notice::copied_notice,
    crate::notice::change_suggested_format,
    crate::native_feel::native_appearance,
    crate::row_context::row_context,
    crate::updater::setting::update_checks,
    crate::updater::setting::set_update_checks,
    crate::updater::setting::check_for_updates,
    crate::feedback::open_about_link,
    crate::updater::update_available,
    crate::updater::update_picker_activity,
    crate::settings_window::open_settings,
    crate::settings_window::finish_paste_guide,
    crate::settings_backup::back_up_settings,
    crate::settings_backup::choose_settings_backup,
    crate::settings_backup::restore_settings_backup,
    crate::settings_backup::cancel_settings_restore,
    crate::paste::permission::paste_permission,
    crate::paste::permission::open_paste_permission,
    crate::paste::permission::drag_paste_app,
    crate::shortcut_setting::set_shortcut,
    crate::sizing::window_size,
    crate::sizing::set_window_size,
    crate::tray::take_search_request,
    crate::tray::alignment_tick,
    crate::sizing::reset_window_position,
    crate::sizing::read_window_position_undo,
    crate::sizing::undo_window_position,
    crate::sizing::dismiss_window_position_undo,
];

/// Every command, the types they name, and the engine's outputs, read once at
/// build time so names never wait for IPC.
#[cfg(test)]
fn contract() -> Value {
    let mut schemas = Schemas {
        input: SchemaSettings::draft2020_12().for_deserialize().into_generator(),
        output: SchemaSettings::draft2020_12().for_serialize().into_generator(),
    };
    // Sent in events rather than returned by a command.
    let events = [
        schemas.output::<crate::preferences::PreferenceChange>(),
        schemas.output::<crate::settings_backup::BackupProblem>(),
        schemas.output::<crate::settings_backup::RestorePreview>(),
    ];
    let commands: Vec<Value> = commands()
        .into_iter()
        .map(|describe| {
            let command = describe(&mut schemas);
            json!({
                "name": command.name,
                "docs": command.docs,
                "args": command.args.into_iter().map(|(name, schema)| json!([name, schema])).collect::<Vec<_>>(),
                "returns": command.returns,
                "error": command.error,
            })
        })
        .collect();
    // What every unset choice means, from the engine's one owner. Formats
    // follow the library and the app, so their defaults are the two constants.
    let mut defaults = json!(crate::preferences::PickerPreferences::defaults());
    if let Some(fields) = defaults.as_object_mut() {
        fields.remove("output");
        fields.remove("key_output");
        // The language the app speaks is reported, never a default to fall back on.
        fields.remove("speaking");
    }
    json!({
        "commands": commands,
        "events": events,
        "input": schemas.input.take_definitions(true),
        "output": closed_codes(schemas.output.take_definitions(true)),
        "constants": {
            "CITATION_FORMS": ibis_core::contract::CitationForm::ALL,
            "OUTPUTS": ibis_engine::outputs::outputs().iter().map(|output| json!({
                "id": output.id,
                "name": output.name,
                "description": output.description,
                "example": output.example,
                "style": matches!(output.output, ibis_core::contract::OutputSpec::Csl { .. }),
                "entries": output.entries,
                "shares": output.exports_records,
            })).collect::<Vec<_>>(),
            "DEFAULT_OUTPUT": ibis_engine::outputs::DEFAULT,
            "DEFAULT_STYLE": ibis_engine::outputs::DEFAULT_STYLE,
            "DEFAULT_PREFERENCES": defaults,
        },
    })
}

/// Close every problem's `code` over the codes the desktop can send, so the
/// interface compares against a closed set and a renamed code is a type error.
#[cfg(test)]
fn closed_codes(mut output: serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    let codes: std::collections::BTreeSet<String> = crate::errors::every_problem()
        .into_iter()
        .map(|problem| problem.code)
        .collect();
    for name in ["Problem", "CommandError"] {
        if let Some(code) = output
            .get_mut(name)
            .and_then(|problem| problem.pointer_mut("/properties/code"))
        {
            *code = json!({ "$ref": "#/$defs/ProblemCode" });
        }
    }
    output.insert(
        "ProblemCode".into(),
        json!({
            "description": "Every code a desktop problem can carry, from the engine's and the app's own failures.",
            "enum": codes,
        }),
    );
    output
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    #[test]
    fn every_command_failure_is_data_with_required_recovery() {
        let contract = super::contract();
        let definitions = contract["output"].as_object().unwrap_or_else(|| panic!("definitions"));
        for command in contract["commands"].as_array().unwrap_or_else(|| panic!("commands")) {
            let mut error = &command["error"];
            if error.is_null() {
                continue;
            }
            if let Some(reference) = error["$ref"].as_str() {
                error = &definitions[reference.trim_start_matches("#/$defs/")];
            }
            assert_ne!(
                error["type"], "string",
                "{} throws a plain string: {error}",
                command["name"]
            );
            let required = error["required"]
                .as_array()
                .unwrap_or_else(|| panic!("{} needs structured error data: {error}", command["name"]));
            for field in ["code", "message", "recovery", "focus"] {
                assert!(
                    required.iter().any(|name| name == field),
                    "{} lacks {field}: {error}",
                    command["name"]
                );
            }
        }
    }

    #[test]
    fn every_code_the_interface_reads_is_closed() {
        let contract = super::contract();
        let definitions = contract["output"].as_object().unwrap_or_else(|| panic!("definitions"));
        for (name, definition) in definitions {
            assert_ne!(
                definition
                    .pointer("/properties/code/type")
                    .and_then(serde_json::Value::as_str),
                Some("string"),
                "{name}.code is an open string; give it a closed type"
            );
        }
        assert!(
            definitions["ProblemCode"]["enum"]
                .as_array()
                .is_some_and(|codes| codes.iter().any(|code| code == "cancelled")),
            "the closed codes come from the desktop's own problems"
        );
    }

    include!("../app_commands.rs");

    /// What Settings may call: saved choices and library names/IDs, never documents or copies.
    const SETTINGS_COMMANDS: &[&str] = &[
        "attachment_text_libraries",
        "set_attachment_text",
        "attachment_text_outside_library",
        "set_attachment_text_outside_library",
        "format_settings",
        "forget_app_format",
        "open_settings_file",
        "picker_preferences",
        "save_picker_preferences",
        "editor_endpoint",
        "set_editor_endpoint",
        "copy_editor_endpoint",
        "delight_view",
        "picker_preferences_saved",
        "window_size",
        "set_window_size",
        "login_item",
        "set_login_item",
        "update_checks",
        "set_update_checks",
        "check_for_updates",
        "open_about_link",
        "paste_permission",
        "open_paste_permission",
        "drag_paste_app",
        "finish_paste_guide",
        "shortcut_status",
        "set_shortcut",
        "configure_shortcuts",
        "back_up_settings",
        "choose_settings_backup",
        "restore_settings_backup",
        "cancel_settings_restore",
        "agent_apps",
        "connect_agent_app",
        "disconnect_agent_app",
        "waiting_proposals",
        "apply_proposal",
        "reject_proposal",
        "reject_app_proposals",
        "cli_status",
        "set_cli_installed",
        "copy_cli_profile",
        "copy_version",
    ];

    /// Only Settings asks about paste permission, changes the global shortcut
    /// or chooses a window size.
    const SETTINGS_ONLY: &[&str] = &[
        "attachment_text_libraries",
        "set_attachment_text",
        "attachment_text_outside_library",
        "set_attachment_text_outside_library",
        "format_settings",
        "forget_app_format",
        "open_settings_file",
        "editor_endpoint",
        "set_editor_endpoint",
        "copy_editor_endpoint",
        "open_about_link",
        "window_size",
        "set_window_size",
        "paste_permission",
        "open_paste_permission",
        "drag_paste_app",
        "finish_paste_guide",
        "set_shortcut",
        "configure_shortcuts",
        "back_up_settings",
        "choose_settings_backup",
        "restore_settings_backup",
        "cancel_settings_restore",
        "agent_apps",
        "connect_agent_app",
        "disconnect_agent_app",
        "waiting_proposals",
        "apply_proposal",
        "reject_proposal",
        "reject_app_proposals",
        "cli_status",
        "set_cli_installed",
        "copy_cli_profile",
        "copy_version",
    ];

    #[test]
    fn command_manifest_matches_the_runtime_registry() {
        let mut schemas = super::Schemas {
            input: schemars::SchemaGenerator::default(),
            output: schemars::SchemaGenerator::default(),
        };
        let registered: Vec<_> = super::commands()
            .into_iter()
            .map(|describe| describe(&mut schemas).name)
            .collect();
        let unique: BTreeSet<_> = registered.iter().copied().collect();
        assert_eq!(unique.len(), registered.len(), "duplicate runtime command");
        assert_eq!(
            unique,
            APP_COMMANDS.iter().copied().collect(),
            "manifest and registry differ"
        );
    }

    #[test]
    fn capabilities_grant_only_their_window_commands() {
        let picker: serde_json::Value = serde_json::from_str(include_str!("../capabilities/default.json"))
            .unwrap_or_else(|e| panic!("invalid picker capability: {e}"));
        let notice: serde_json::Value = serde_json::from_str(include_str!("../capabilities/notice.json"))
            .unwrap_or_else(|e| panic!("invalid notice capability: {e}"));
        let settings: serde_json::Value = serde_json::from_str(include_str!("../capabilities/settings.json"))
            .unwrap_or_else(|e| panic!("invalid settings capability: {e}"));
        for (capability, label) in [(&picker, "main"), (&notice, "notice"), (&settings, "settings")] {
            assert_eq!(capability["windows"], serde_json::json!([label]));
            assert!(capability.get("webviews").is_none());
            assert!(capability.get("remote").is_none());
            assert!(capability.get("local").is_none_or(|value| value == true));
        }
        assert_eq!(
            notice["permissions"],
            serde_json::json!([
                "core:event:allow-listen",
                "core:event:allow-unlisten",
                "allow-copied-notice",
                "allow-change-suggested-format"
            ])
        );
        let mut settings_expected = [
            "core:event:allow-listen",
            "core:event:allow-unlisten",
            "core:app:allow-version",
            "core:window:allow-start-dragging",
            "core:window:allow-destroy",
        ]
        .map(str::to_owned)
        .to_vec();
        settings_expected.extend(
            SETTINGS_COMMANDS
                .iter()
                .map(|command| format!("allow-{}", command.replace('_', "-"))),
        );
        assert_eq!(settings["permissions"], serde_json::json!(settings_expected));
        let mut expected = [
            "core:event:allow-listen",
            "core:event:allow-unlisten",
            "core:window:allow-internal-toggle-maximize",
            "core:window:allow-start-dragging",
        ]
        .map(str::to_owned)
        .to_vec();
        expected.extend(
            APP_COMMANDS
                .iter()
                .filter(|command| {
                    !matches!(**command, "copied_notice" | "change_suggested_format")
                        && !SETTINGS_ONLY.contains(command)
                })
                .map(|command| format!("allow-{}", command.replace('_', "-"))),
        );
        assert_eq!(picker["permissions"], serde_json::json!(expected));
    }

    #[test]
    fn window_commands_are_rejected_before_the_handler() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let app = tauri::test::mock_builder()
            .invoke_handler(move |invoke| {
                observed.fetch_add(1, Ordering::SeqCst);
                invoke.resolver.resolve(true);
                true
            })
            .build(tauri::generate_context!(test = true))
            .unwrap_or_else(|e| panic!("cannot build ACL fixture: {e}"));
        let local = if cfg!(windows) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        };
        for label in [
            "notice",
            "main",
            "settings",
            "unlisted",
            "notice-extra",
            "main-extra",
            "settings-extra",
        ] {
            let window = tauri::WebviewWindowBuilder::new(&app, label, Default::default())
                .build()
                .unwrap_or_else(|e| panic!("cannot build window fixture: {e}"));
            for origin in [
                local,
                "https://example.invalid",
                "http://tauri.localhost.example.invalid",
            ] {
                for command in APP_COMMANDS
                    .iter()
                    .copied()
                    .chain(["", "unregistered_command", "copied_notice_extra"])
                {
                    let before = calls.load(Ordering::SeqCst);
                    let response = tauri::test::get_ipc_response(
                        &window,
                        tauri::webview::InvokeRequest {
                            cmd: command.into(),
                            callback: tauri::ipc::CallbackFn(0),
                            error: tauri::ipc::CallbackFn(1),
                            url: origin.parse().unwrap_or_else(|e| panic!("invalid fixture URL: {e}")),
                            body: Default::default(),
                            headers: Default::default(),
                            invoke_key: tauri::test::INVOKE_KEY.into(),
                        },
                    );
                    let allowed = origin == local
                        && APP_COMMANDS.contains(&command)
                        && match label {
                            "main" => {
                                !matches!(command, "copied_notice" | "change_suggested_format")
                                    && !SETTINGS_ONLY.contains(&command)
                            }
                            "notice" => matches!(command, "copied_notice" | "change_suggested_format"),
                            "settings" => SETTINGS_COMMANDS.contains(&command),
                            _ => false,
                        };
                    assert_eq!(response.is_ok(), allowed, "{label}: {command} from {origin}");
                    assert_eq!(
                        calls.load(Ordering::SeqCst),
                        before + usize::from(allowed),
                        "denied calls must not reach handlers"
                    );
                }
            }
        }
    }

    /// Words come only from the catalogue (decision 0064): no type the web view,
    /// the engine's contract or the command line's JSON carries holds a
    /// sentence for a person to read. MCP's text is its own front end's words.
    /// A refusal is a `Problem`, a reason a closed code, and each front end
    /// words it. A string field named like words is either data, listed with
    /// why, or a known offender that may only leave the list.
    #[test]
    fn no_contract_type_carries_words_for_a_person() {
        use serde_json::Value;
        const DATA: &[(&str, &str)] = &[
            // The person's own library, the citation Ibis wrote, or a name.
            ("Annotation.page_label", "the page label the PDF prints"),
            ("Annotation.text", "the highlighted text from the PDF"),
            ("Attachment.title", "the attachment's own title"),
            ("CitationResult.text", "the citation as written"),
            ("ClosestMatch.title", "the reference's title"),
            ("Copied.text", "the citation as written"),
            ("DisplayRun.text", "a run of the citation as written"),
            ("LocatorInput.text", "the locator as the person typed it"),
            ("Note.text", "the reference's own note"),
            ("NoticeMessage.text", "one line of the citation that was copied"),
            ("Preview.text", "the citation as written"),
            ("ReceiptSummary.text", "the citation that was copied"),
            ("SearchQuery.text", "what the person typed"),
            ("SearchResult.title", "the reference's title"),
            ("SessionBibliography.text", "the bibliography as written"),
            ("TextRun.text", "a run of the citation as written"),
            (
                "WaitingProposal.lines",
                "the proposed fields and tags, as `name: value`",
            ),
            ("cli:ProposalPreview.lines", "the proposed fields and tags"),
        ];
        const OFFENDERS: &[(&str, &str)] = &[
            ("LibraryStatus.error", "the status owner (0064 step 3)"),
            ("LibraryStatus.note", "the status owner (0064 step 3)"),
            ("LibraryStatus.settings_notice", "the status owner (0064 step 3)"),
            ("ReadingNotice.why", "the status owner (0064 step 3)"),
            (
                "ShortcutStatus.error",
                "a closed code, with Settings › Shortcuts' rewrite",
            ),
            ("cli:AgentChange.detail", "agent apps' outcomes as codes"),
            ("cli:ProjectAssessment.message", "a project conflict as a Problem"),
            ("Preview.problem", "a copy of `failure`; the picker can word that"),
            ("Preview.project_refusal", "a project conflict as a Problem"),
            ("AgentApp.detail", "agent apps' states and steps as codes"),
        ];
        const WORDS: &[&str] = &[
            "body",
            "caption",
            "description",
            "detail",
            "error",
            "explanation",
            "hint",
            "label",
            "lines",
            "message",
            "note",
            "notes",
            "notice",
            "problem",
            "reason",
            "refusal",
            "sentence",
            "summary",
            "text",
            "title",
            "why",
            "words",
        ];
        fn is_text(schema: &Value) -> bool {
            if schema.get("enum").is_some() || schema.get("const").is_some() {
                return false;
            }
            let kinds: Vec<&str> = match &schema["type"] {
                Value::String(kind) => vec![kind],
                Value::Array(kinds) => kinds.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            };
            if kinds.contains(&"string") {
                return true;
            }
            if kinds.contains(&"array") {
                return is_text(&schema["items"]);
            }
            ["anyOf", "oneOf"].iter().any(|keyword| {
                schema[keyword]
                    .as_array()
                    .is_some_and(|variants| variants.iter().any(is_text))
            })
        }
        fn walk(name: &str, schema: &Value, found: &mut BTreeSet<String>) {
            if let Some(properties) = schema["properties"].as_object() {
                for (field, value) in properties {
                    if field.split('_').any(|word| WORDS.contains(&word)) && is_text(value) {
                        found.insert(format!("{name}.{field}"));
                    }
                }
            }
            for keyword in ["oneOf", "anyOf", "allOf"] {
                for variant in schema[keyword].as_array().into_iter().flatten() {
                    walk(name, variant, found);
                }
            }
        }
        let contract = super::contract();
        let engine: Value = serde_json::from_str(include_str!("../../../../fixtures/engine-contract.schema.json"))
            .unwrap_or_else(|e| panic!("engine contract: {e}"));
        let mut found = BTreeSet::new();
        for definitions in [&contract["output"], &contract["input"], &engine["$defs"]] {
            for (name, schema) in definitions.as_object().into_iter().flatten() {
                walk(name, schema, &mut found);
            }
        }
        // Engine types only the command line serialises, by their own labels.
        for (name, schema) in [
            (
                "cli:AgentChange",
                schemars::schema_for!(ibis_engine::agent_apps::Change),
            ),
            (
                "cli:ProjectAssessment",
                schemars::schema_for!(ibis_engine::ProjectAssessment),
            ),
            (
                "cli:ProposalPreview",
                schemars::schema_for!(ibis_engine::write::Preview),
            ),
        ] {
            walk(name, &schema.to_value(), &mut found);
        }
        let listed: BTreeSet<String> = DATA
            .iter()
            .chain(OFFENDERS)
            .map(|(field, _)| (*field).to_owned())
            .collect();
        let new: Vec<_> = found.difference(&listed).collect();
        assert!(
            new.is_empty(),
            "words for a person in a contract type; send a Problem, a Message or a closed code instead: {new:?}"
        );
        let gone: Vec<_> = listed.difference(&found).collect();
        assert!(
            gone.is_empty(),
            "no longer in any contract type; take them off the list: {gone:?}"
        );
    }

    /// Run by `node scripts/gen.mjs`, which writes `src/bindings.ts` from it.
    #[test]
    #[ignore = "writes the desktop contract for gen.mjs"]
    fn export_contract() {
        let directory = std::env::var_os("IBIS_GEN_DIR").unwrap_or_else(|| panic!("set IBIS_GEN_DIR"));
        let text = serde_json::to_string_pretty(&super::contract()).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(std::path::Path::new(&directory).join("desktop-contract.json"), text)
            .unwrap_or_else(|e| panic!("cannot write desktop contract: {e}"));
    }
}
