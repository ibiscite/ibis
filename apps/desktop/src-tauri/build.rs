mod app_commands;

fn main() {
    println!("cargo:rerun-if-changed=app_commands.rs");
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(app_commands::APP_COMMANDS)),
    )
    .unwrap_or_else(|e| panic!("cannot build desktop command permissions: {e}"));
}
