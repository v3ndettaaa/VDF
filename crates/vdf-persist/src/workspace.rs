//! Per-document workspace: where autosave state lives (command log,
//! checkpoints, blobs). The source PDF itself is never touched by opening.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Version of the workspace layout/manifest schema.
pub const WORKSPACE_SCHEMA_VERSION: u32 = 1;

/// Platform-application user data directory
/// (`$XDG_DATA_HOME`/`~/.local/share` on Linux, `%APPDATA%` on Windows).
pub fn app_data_dir() -> PathBuf {
    if cfg!(target_os = "windows")
        && let Some(appdata) = std::env::var_os("APPDATA")
    {
        return PathBuf::from(appdata).join("vdf");
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(xdg).join("vdf");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".local").join("share").join("vdf")
}

/// Identity of one open document's workspace: derived from the source file,
/// never from secrets (no passwords are stored — MASTER_PLAN.md §12).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub schema_version: u32,
    /// Absolute path of the source PDF.
    pub source_path: String,
    /// Hex SHA-256 of the source bytes at open time (computed from M1).
    pub source_sha256: String,
    /// Document revision the workspace has durably recorded.
    pub revision: u64,
}

impl WorkspaceManifest {
    pub fn new(source_path: impl Into<String>, source_sha256: impl Into<String>) -> Self {
        Self {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            source_path: source_path.into(),
            source_sha256: source_sha256.into(),
            revision: 0,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("manifest serialization cannot fail")
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        let m: Self = serde_json::from_str(s).map_err(|e| e.to_string())?;
        if m.schema_version > WORKSPACE_SCHEMA_VERSION {
            return Err(format!(
                "workspace schema {} is newer than supported {}",
                m.schema_version, WORKSPACE_SCHEMA_VERSION
            ));
        }
        Ok(m)
    }
}

/// Paths for one document's workspace.
#[derive(Debug, Clone)]
pub struct WorkspacePaths {
    pub root: PathBuf,
}

impl WorkspacePaths {
    /// `<data>/vdf/workspaces/<key>`
    pub fn for_key(key: &str) -> Self {
        Self {
            root: app_data_dir().join("workspaces").join(key),
        }
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.root.join("manifest.json")
    }

    pub fn commands_log_path(&self) -> PathBuf {
        self.root.join("commands.log")
    }

    pub fn checkpoints_dir(&self) -> PathBuf {
        self.root.join("checkpoints")
    }

    pub fn blobs_dir(&self) -> PathBuf {
        self.root.join("blobs")
    }

    /// Creates the workspace directory skeleton.
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.root.join("checkpoints"))?;
        std::fs::create_dir_all(self.root.join("blobs"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trip_and_version_guard() {
        let m = WorkspaceManifest::new("/tmp/book.pdf", "abc123");
        let parsed = WorkspaceManifest::from_json(&m.to_json()).unwrap();
        assert_eq!(parsed, m);
        assert_eq!(parsed.schema_version, WORKSPACE_SCHEMA_VERSION);

        let future = r#"{"schema_version":999,"source_path":"","source_sha256":"","revision":0}"#;
        assert!(
            WorkspaceManifest::from_json(future).is_err(),
            "future schemas must be rejected, not guessed"
        );
    }

    #[test]
    fn workspace_paths_layout() {
        let w = WorkspacePaths::for_key("deadbeef");
        assert!(w.root.to_string_lossy().contains("workspaces"));
        assert!(w.manifest_path().ends_with("manifest.json"));
        assert!(w.commands_log_path().ends_with("commands.log"));
        assert!(w.checkpoints_dir().ends_with("checkpoints"));
        assert!(w.blobs_dir().ends_with("blobs"));
    }
}
