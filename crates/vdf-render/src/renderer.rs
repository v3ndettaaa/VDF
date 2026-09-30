//! Tile renderer abstraction + the real MuPDF-backed implementation.
//!
//! The scheduler is generic over [`TileRenderer`] so tests can use a fake
//! while the app plugs in MuPDF. Worker (cloned) contexts are a MuPDF
//! implementation detail: [`MupdfTileRenderer`] lazily creates one per
//! calling thread (MASTER_PLAN.md §8 — each render worker gets its own
//! `fz_clone_context`).

use std::cell::RefCell;
use std::sync::Arc;

use vdf_core::{Rect, TileKey, VdfResult};
use vdf_pdf::PageDisplayList;

/// Request for one tile.
pub struct TileJob {
    pub doc_rev: u64,
    pub key: TileKey,
    /// Continuous zoom (the key carries the quantized step).
    pub zoom: f64,
    /// Region of the rotated, zoomed page in device px.
    pub region: Rect,
    pub width: u32,
    pub height: u32,
    pub display_list: Arc<PageDisplayList>,
}

/// Produces pixels for one tile. Implementations must be thread-safe:
/// `render` may be called concurrently (display lists are MuPDF's
/// concurrency unit — MASTER_PLAN.md §8).
pub trait TileRenderer: Send + Sync {
    fn render(&self, job: &TileJob) -> VdfResult<Vec<u8>>;
}

/// MuPDF-backed renderer. One worker context is created per rendering
/// thread on first use; workers must be gone before the engine drops
/// (guaranteed by scheduler shutdown ordering in `vdf-app`).
pub struct MupdfTileRenderer {
    doc: Arc<vdf_pdf::PdfDocument>,
    engine: Arc<vdf_pdf::MupdfEngine>,
}

thread_local! {
    static WORKER_CONTEXT: RefCell<Option<mupdf_sys::RawContext>> = const { RefCell::new(None) };
}

impl MupdfTileRenderer {
    pub fn new(engine: Arc<vdf_pdf::MupdfEngine>, doc: Arc<vdf_pdf::PdfDocument>) -> Self {
        Self { doc, engine }
    }

    /// Raw pointer of this thread's cloned worker context. The TLS slot
    /// owns the context; the pointer is valid until the thread exits.
    fn worker_ptr(&self) -> VdfResult<*mut mupdf_sys::raw::fz_context> {
        WORKER_CONTEXT.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_none() {
                *slot = Some(self.engine.worker_context()?);
            }
            Ok(slot.as_ref().expect("just created").ptr)
        })
    }
}

impl TileRenderer for MupdfTileRenderer {
    fn render(&self, job: &TileJob) -> VdfResult<Vec<u8>> {
        let ptr = self.worker_ptr()?;
        // A drop-guard-free view of the TLS-owned context: the temporary
        // RawContext below never drops (the TLS slot owns the real drop).
        let worker = std::mem::ManuallyDrop::new(mupdf_sys::RawContext { ptr });
        self.doc
            .render_region(
                &worker,
                &job.display_list,
                job.zoom,
                job.key.rotation,
                job.region,
            )
            .map_err(vdf_core::VdfError::from)
    }
}

/// Deterministic fake renderer for tests.
pub struct FakeTileRenderer {
    pub rendered: std::sync::atomic::AtomicU64,
    pub fail: std::sync::atomic::AtomicBool,
}

impl FakeTileRenderer {
    pub fn new() -> Self {
        Self {
            rendered: std::sync::atomic::AtomicU64::new(0),
            fail: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn rendered_count(&self) -> u64 {
        self.rendered.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for FakeTileRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl TileRenderer for FakeTileRenderer {
    fn render(&self, job: &TileJob) -> VdfResult<Vec<u8>> {
        use std::sync::atomic::Ordering;
        self.rendered.fetch_add(1, Ordering::Relaxed);
        if self.fail.load(Ordering::Relaxed) {
            return Err(vdf_core::VdfError::Render("fake renderer failure".into()));
        }
        let mut h = job.key.page.0 as u64;
        h = (h ^ job.key.x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h = (h ^ job.key.y as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let (r, g, b) = (
            (h & 0xFF) as u8,
            ((h >> 8) & 0xFF) as u8,
            ((h >> 16) & 0xFF) as u8,
        );
        let count = (job.width as usize) * (job.height as usize);
        let mut buf = Vec::with_capacity(count * 4);
        for _ in 0..count {
            buf.extend_from_slice(&[r, g, b, 255]);
        }
        Ok(buf)
    }
}
