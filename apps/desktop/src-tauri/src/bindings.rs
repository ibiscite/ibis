//! One command registry drives both the runtime handler and generated bindings.

pub(crate) fn commands() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::new()
        .commands(tauri_specta::collect_commands![
            crate::library::commands::choose_library,
            crate::library::commands::reload_library,
            crate::library::commands::use_zotero,
            crate::setup::find_sources,
            crate::setup::open_found_library,
            crate::library::commands::use_zotero_libraries,
            crate::library::commands::zotero_groups,
            crate::library::commands::library_status,
            crate::library::commands::search,
            crate::copy::copy_citation,
            crate::copy::citation_text,
            crate::copy::copy_format,
            crate::copy::csl_items,
            crate::copy::copy_formatted,
            crate::preferences::picker_preferences,
            crate::preferences::set_picker_preferences,
            crate::login_item::login_item,
            crate::login_item::set_login_item,
            crate::picker::hide_picker,
            crate::picker::set_pinned,
            crate::picker::quit,
            crate::picker::shortcut_status,
            crate::notice::show_copied_notice,
            crate::notice::copied_notice,
            crate::native_feel::native_appearance,
            crate::row_context::row_context
        ])
        // Match Tauri's existing Promise rejection behavior.
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        // usize counts/limits already cross the JSON bridge as JS numbers.
        // Library input is capped at 64 MiB and search results at 200.
        .dangerously_cast_bigints_to_number()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    include!("../app_commands.rs");

    fn destination() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts")
    }

    fn generated() -> String {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("cannot create contract directory: {e}"));
        let path = directory.path().join("bindings.ts");
        super::commands()
            .export(specta_typescript::Typescript::default(), &path)
            .unwrap_or_else(|e| panic!("cannot generate desktop contract: {e}"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read generated contract: {e}"));
        // The exporter emits trailing whitespace in documented enum unions.
        // Normalize formatting only, keeping the generated schema untouched.
        let lines = text.trim_end().lines().map(str::trim_end).collect::<Vec<_>>();
        format!("{}\n", lines.join("\n"))
    }

    #[test]
    fn generated_contract_is_current() {
        let checked_in = std::fs::read_to_string(destination()).unwrap_or_default();
        assert_eq!(
            checked_in.replace("\r\n", "\n"),
            generated().replace("\r\n", "\n"),
            "stale desktop contract; run cargo test -p ibis-desktop bindings::tests::export_contract -- --ignored"
        );
    }

    #[test]
    fn command_manifest_matches_the_runtime_registry() {
        struct Registry;
        impl tauri_specta::LanguageExt for Registry {
            type Error = std::io::Error;

            fn export(
                self,
                config: &tauri_specta::BuilderConfiguration,
                _: &std::path::Path,
            ) -> Result<(), Self::Error> {
                let registered = config
                    .commands
                    .iter()
                    .map(|command| command.name())
                    .collect::<BTreeSet<_>>();
                assert_eq!(registered.len(), config.commands.len(), "duplicate runtime command");
                assert_eq!(
                    APP_COMMANDS.len(),
                    registered.len(),
                    "duplicate or missing manifest command"
                );
                assert_eq!(registered, APP_COMMANDS.iter().copied().collect());
                Ok(())
            }
        }
        super::commands()
            .export(Registry, "unused")
            .unwrap_or_else(|e| panic!("cannot inspect command registry: {e}"));
    }

    #[test]
    fn capabilities_grant_only_their_window_commands() {
        let picker: serde_json::Value = serde_json::from_str(include_str!("../capabilities/default.json"))
            .unwrap_or_else(|e| panic!("invalid picker capability: {e}"));
        let notice: serde_json::Value = serde_json::from_str(include_str!("../capabilities/notice.json"))
            .unwrap_or_else(|e| panic!("invalid notice capability: {e}"));
        for (capability, label) in [(&picker, "main"), (&notice, "notice")] {
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
                .filter(|command| **command != "copied_notice")
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
        for label in ["notice", "main", "unlisted", "notice-extra", "main-extra"] {
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
                            "main" => command != "copied_notice",
                            "notice" => command == "copied_notice",
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

    #[test]
    #[ignore = "explicitly regenerate checked-in TypeScript, never during checks"]
    fn export_contract() {
        std::fs::write(destination(), generated()).unwrap_or_else(|e| panic!("cannot write desktop contract: {e}"));
    }
}
