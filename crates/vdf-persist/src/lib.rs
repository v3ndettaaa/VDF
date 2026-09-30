//! vdf-persist — persistence, save, recovery, export (full implementation in M5).
//!
//! M0 establishes the API boundaries that must exist from day one so later
//! milestones never reshuffle the persistence contract:
//! - [`atomic`] — the write discipline every save obeys: temp file → flush →
//!   fsync → atomic rename. If interrupted at any point, the original stays
//!   valid.
//! - [`workspace`] — per-document workspace paths + the manifest schema.
//!   Autosave appends a command log here; it never rewrites the PDF.
//!
//! Still to come (M5): IncrementalWriter, FullWriter, Validator,
//! RecoveryScanner, command-log records, checkpoints, ExportManager.

pub mod atomic;
pub mod workspace;

pub use atomic::atomic_write;
pub use workspace::{WorkspaceManifest, WorkspacePaths};
