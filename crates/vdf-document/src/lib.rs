//! vdf-document — the VDF document model.
//!
//! M0 scope (MASTER_PLAN.md §16): foundations only — document/page/object
//! structures, the command trait, and undo/redo history with inverse-data
//! semantics (no whole-document snapshots). The spatial index, serialization
//! schema/migrations, and the full object-type payloads land in later
//! milestones as their features are built.

pub mod command;
pub mod document;
pub mod object;

pub use command::{DocumentCommand, History};
pub use document::{AddObject, Document, PageModel, SetObjectOpacity};
pub use object::{ObjectModel, ObjectType};

/// Serialization schema version for everything this crate persists
/// (snapshots, command log records, manifests).
pub const SCHEMA_VERSION: u32 = 1;
