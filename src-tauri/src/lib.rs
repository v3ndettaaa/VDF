//! vdf-app — the Tauri v2 shell (MASTER_PLAN.md §5).
//!
//! Owns everything platform-facing: the window, native dialogs, the JSON
//! command surface, and the custom URI-scheme protocols that carry tiles
//! and thumbnails as raw bytes. The core crates never learn about any of
//! this.
//!
//! M1 command surface (viewer):
//! - `vdf_app_info`, `vdf_open_file` (M0 dialog helper)
//! - `vdf_open_document` / `vdf_close_document`
//! - `vdf_viewport` / `vdf_poll` / `vdf_pan` / `vdf_zoom` / `vdf_fit_width`
//! - `vdf_set_page_mode` / `vdf_rotate` / `vdf_goto_page` / `vdf_outline`
//! - `vdf_request_thumbnails`
//! - protocols: `vdf-tile://d{doc}/{key}` and `vdf-thumb://d{doc}/{page}/{w}`

pub mod commands;
mod protocols;
pub mod state;

use tauri_plugin_dialog::DialogExt;

use serde::Serialize;
use state::AppCore;
use vdf_diag::MetricsRegistry;

/// Process-wide diagnostics registry (fed by subsystems from M1 on).
pub struct Diag(pub MetricsRegistry);

#[derive(Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: String,
    pub platform: String,
    pub arch: String,
    /// Document serialization schema this build speaks.
    pub schema_version: u32,
    /// Diagnostics counters registered so far (grows as subsystems land).
    pub metrics_registered: usize,
}

#[tauri::command]
fn vdf_app_info(state: tauri::State<Diag>) -> AppInfo {
    AppInfo {
        name: "VDF",
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        schema_version: vdf_document::SCHEMA_VERSION,
        metrics_registered: state.0.snapshot().len(),
    }
}

#[derive(Serialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub extension: String,
}

/// Opens a native file dialog and reports the picked file's name, path and
/// size. Reading/parsing happens in `vdf_open_document`.
#[tauri::command]
async fn vdf_open_file(app: tauri::AppHandle) -> Result<Option<FileInfo>, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("PDF Document", &["pdf"])
        .add_filter("All files", &["*"])
        .blocking_pick_file();

    let Some(file) = picked else {
        return Ok(None); // user cancelled — not an error
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    Ok(Some(FileInfo {
        name,
        path: path.to_string_lossy().into_owned(),
        size_bytes: meta.len(),
        extension,
    }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_core = AppCore::new().expect("init AppCore");
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(app_core)
        .manage(Diag(MetricsRegistry::new()))
        .invoke_handler(tauri::generate_handler![
            vdf_app_info,
            vdf_open_file,
            commands::vdf_open_document,
            commands::vdf_close_document,
            commands::vdf_viewport,
            commands::vdf_poll,
            commands::vdf_pan,
            commands::vdf_zoom,
            commands::vdf_fit_width,
            commands::vdf_set_page_mode,
            commands::vdf_rotate,
            commands::vdf_goto_page,
            commands::vdf_outline,
            commands::vdf_request_thumbnails,
            commands::vdf_tile_meta,
            commands::vdf_page_label,
        ])
        .setup(|_app| Ok(()));
    let builder = protocols::register(builder);
    builder
        .run(tauri::generate_context!())
        .expect("error while running VDF");
}
