mod app_commands;

fn main() -> Result<(), String> {
    // Build metadata only; never consult a checkout or the network.
    let date = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=IBIS_BUILD_DATE={}", date.trim());
    println!("cargo:rerun-if-changed=app_commands.rs");
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(app_commands::APP_COMMANDS)),
    )
    .map_err(|e| format!("cannot build desktop command permissions: {e}"))
}
