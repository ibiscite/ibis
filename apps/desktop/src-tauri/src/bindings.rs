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
    crate::copy::copy_file_citation,
    crate::project::project_status,
    crate::project::retry_project,
    crate::project::choose_project,
    crate::project::stop_project,
    crate::project::copy_project_entry,
    crate::writing_projects::writing_projects,
    crate::writing_projects::edit_writing_project,
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
    crate::destination::picker_destination,
    crate::destination::remember_app_output,
    crate::login_item::login_item,
    crate::login_item::set_login_item,
    crate::picker::hide_picker,
    crate::picker::picker_presentation,
    crate::picker::set_pinned,
    crate::picker::picker_moving,
    crate::picker::quit,
    crate::picker::shortcut_status,
    crate::picker::wayland::configure_shortcuts,
    crate::notice::show_copied_notice,
    crate::notice::copied_notice,
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
    json!({
        "commands": commands,
        "events": events,
        "input": schemas.input.take_definitions(true),
        "output": schemas.output.take_definitions(true),
        "constants": {
            "OUTPUTS": crate::output::choices(),
            "DEFAULT_OUTPUT": ibis_engine::outputs::DEFAULT,
            "DEFAULT_STYLE": ibis_engine::outputs::DEFAULT_STYLE,
        },
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    include!("../app_commands.rs");

    /// What the Settings window may call: saved choices, never the library or copies.
    const SETTINGS_COMMANDS: &[&str] = &[
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
    ];

    /// Only Settings asks about paste permission, changes the global shortcut
    /// or chooses a window size.
    const SETTINGS_ONLY: &[&str] = &[
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
                "allow-copied-notice"
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
            "core:window:allow-is-visible",
            "core:window:allow-start-dragging",
        ]
        .map(str::to_owned)
        .to_vec();
        expected.extend(
            APP_COMMANDS
                .iter()
                .filter(|command| **command != "copied_notice" && !SETTINGS_ONLY.contains(command))
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
                            "main" => command != "copied_notice" && !SETTINGS_ONLY.contains(&command),
                            "notice" => command == "copied_notice",
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
