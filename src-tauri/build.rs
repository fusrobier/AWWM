//! Generates the Tauri context and restricts the frontend to the explicit command list
//! (each command must also be granted in `capabilities/default.json`).

const COMMANDS: &[&str] = &[
    "get_snapshot",
    "list_windows",
    "pin_window",
    "unpin_window",
    "set_window_opacity",
    "unpin_all",
    "activate_window",
    "set_management_enabled",
    "save_settings",
    "reset_settings",
    "apply_rules_now",
    "get_window_icon",
    "get_app_info",
    "get_key_names",
    "open_location",
    "open_documentation",
    "export_settings",
    "import_settings",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )?;
    Ok(())
}
