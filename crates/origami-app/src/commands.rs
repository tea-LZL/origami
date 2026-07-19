use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    name: &'static str,
    core_version: &'static str,
    shell_version: &'static str,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "Origami",
        core_version: origami_core::version(),
        shell_version: env!("CARGO_PKG_VERSION"),
    }
}
