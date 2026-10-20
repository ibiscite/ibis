//! One command registry drives both the runtime handler and generated bindings.

pub(crate) fn commands() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::new()
        .commands(tauri_specta::collect_commands![
            crate::library::choose_library,
            crate::library::reload_library,
            crate::library::use_zotero,
            crate::setup::find_sources,
            crate::setup::open_found_library,
            crate::library::use_zotero_libraries,
            crate::library::zotero_groups,
            crate::library::library_status,
            crate::library::search,
            crate::copy::copy_citation,
            crate::copy::copy_format,
            crate::copy::csl_items,
            crate::copy::copy_formatted,
            crate::preferences::picker_preferences,
            crate::preferences::set_picker_preferences,
            crate::picker::hide_picker,
            crate::picker::set_pinned,
            crate::picker::quit,
            crate::picker::shortcut_status,
            crate::notice::show_copied_notice,
            crate::notice::copied_notice,
            crate::native_feel::native_appearance
        ])
        // Match Tauri's existing Promise rejection behavior.
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        // usize counts/limits already cross the JSON bridge as JS numbers.
        // Library input is capped at 64 MiB and search results at 200.
        .dangerously_cast_bigints_to_number()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

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
    #[ignore = "explicitly regenerate checked-in TypeScript, never during checks"]
    fn export_contract() {
        std::fs::write(destination(), generated()).unwrap_or_else(|e| panic!("cannot write desktop contract: {e}"));
    }
}
