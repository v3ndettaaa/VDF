//! vdf-app — the Tauri v2 shell (MASTER_PLAN.md §5).
//!
//! Owns everything platform-facing: the window, native dialogs, the JSON
//! command surface, and (from M1) the custom URI-scheme protocol handlers
//! that carry tiles and thumbnails as raw bytes. The core crates never learn
//! about any of this.
//!
//! M0 command surface (deliberately tiny — no PDF parsing happens yet):
//! - `vdf_app_info` → build/platform metadata
//! - `vdf_open_file` → native pick dialog, returns name + size only

use serde::Serialize;
use tauri_plugin_dialog::DialogExt;

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
    pub size_bytes: u64,
    pub extension: String,
}

/// Opens a native file dialog and reports the picked file's name and size.
/// M0 contract: this is the *entire* interaction with the file — no reading
/// of contents, no parsing, no rendering (MASTER_PLAN.md §16, M0 scope).
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
        size_bytes: meta.len(),
        extension,
    }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Diag(MetricsRegistry::new()))
        .invoke_handler(tauri::generate_handler![vdf_app_info, vdf_open_file])
        .run(tauri::generate_context!())
        .expect("error while running VDF");
}
