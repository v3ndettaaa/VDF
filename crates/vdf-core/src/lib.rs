//! vdf-core — shared low-level types for all VDF crates.
//!
//! Ownership rules (MASTER_PLAN.md §5):
//! - depends on nothing outside `std` (+ `thiserror` for error derives)
//! - no I/O, no traits with side effects, no PDF knowledge
//! - every other VDF crate may depend on this one; this one depends on none
//!
//! Contents:
//! - [`error`] — the common typed error [`VdfError`]
//! - [`id`] — persistent identity types ([`ObjectId`], [`PageId`], generators)
//! - [`units`] — newtyped units and zoom/rotation types
//! - [`geometry`] — f64 points, rects, and 2D affine transforms
//! - [`tile`] — tile cache keys and render generations (the stale-render guard)

pub mod error;
pub mod geometry;
pub mod id;
pub mod tile;
pub mod units;

pub use error::{VdfError, VdfResult};
pub use geometry::{Affine2, Point, Rect};
pub use id::{DocHandle, DocumentId, IdGenerator, ObjectId, PageId, Revision};
pub use tile::{RenderGeneration, TileKey};
pub use units::{CssPixels, DevicePixels, PageIndex, PdfPoints, Rotation, ZoomFactor};
