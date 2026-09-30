//! vdf-render — tile rendering, scheduling, caching, and transport.
//!
//! M1 delivers the working pipeline (MASTER_PLAN.md §8, §10):
//! - [`layout`] — page layout math for the page modes
//! - [`zoom`] — the central focal-point `DocumentZoomController`
//! - [`cache`] — LRU tile cache with memory budgets + generation guard
//! - [`scheduler`] — viewport → prioritized tile jobs → worker pool →
//!   generation-checked results into cache + transport
//! - [`renderer`] — `TileRenderer` trait + the MuPDF-backed implementation
//! - [`transport`] — the render transport abstraction (M0) with the stale
//!   drop rule

pub mod cache;
pub mod layout;
pub mod renderer;
pub mod scheduler;
pub mod transport;
pub mod zoom;

pub use cache::{MemoryBudgets, TileCache};
pub use layout::{DocumentLayout, PageMode, PageRect, compute_layout, page_view_size};
pub use renderer::{FakeTileRenderer, MupdfTileRenderer, TileJob, TileRenderer};
pub use scheduler::{RenderScheduler, SchedulerConfig, TILE_PX, ViewportState};
pub use transport::{FinishedTile, MemoryTransport, RenderTransport, TransportStats};
pub use zoom::{DocumentZoomController, ZOOM_MAX, ZOOM_MIN};
