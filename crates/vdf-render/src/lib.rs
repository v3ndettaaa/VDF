//! vdf-render — tile rendering, scheduling, caching, and transport
//! (scheduler + caches arrive in M1).
//!
//! M0 establishes the two boundary traits everything else renders through:
//! - [`transport::RenderTransport`] — how finished tiles reach the compositor.
//!   The document renderer knows nothing about Tauri events, PNG encoding,
//!   DOM, Canvas, or browser APIs (MASTER_PLAN.md §6).
//! - [`rasterizer::Rasterizer`] — how a tile's pixels get produced, so tile
//!   logic is testable without MuPDF (the real `MupdfRasterizer` lands in M1;
//!   [`rasterizer::SyntheticRasterizer`] is the deterministic test fake).

pub mod rasterizer;
pub mod transport;

pub use rasterizer::{Rasterizer, SyntheticRasterizer};
pub use transport::{FinishedTile, MemoryTransport, RenderTransport, TransportStats};
