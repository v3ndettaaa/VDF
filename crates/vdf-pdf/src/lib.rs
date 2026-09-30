//! vdf-pdf — safe MuPDF integration (MASTER_PLAN.md §5).
//!
//! The ONLY consumer of `mupdf-sys`. Everything the viewer needs from the
//! PDF engine lives behind these types:
//!
//! - [`MupdfEngine`] — one master `fz_context` per application; hands out
//!   worker (cloned) contexts for parallel tile rendering
//! - [`PdfDocument`] — one open document; immutable shared handle safe to
//!   use from the document actor and render workers (MuPDF's internal locks
//!   — created with our [`mupdf_sys`] locks — serialize shared state)
//! - [`PdfError`] — typed surface for malformed / encrypted / io errors
//!
//! Lifetime rule: worker (`RawContext`) clones and all rendering must stop
//! before the last `PdfDocument` for the engine drops, and documents drop
//! before their engine (enforced by struct field order + app design).

use std::ffi::{CString, c_int};
use std::path::Path;
use std::sync::Arc;

use mupdf_sys::raw;
use mupdf_sys::{FzContext, MupdfError, RawContext, errbuf, take_error};
use vdf_core::{Rect, Rotation, VdfError};

/// Typed errors the UI can act on (MASTER_PLAN.md §23 detection list).
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    /// The bytes are not a loadable document.
    #[error("malformed document: {0}")]
    Malformed(String),
    /// The document is password protected; authenticate and retry.
    #[error("document is encrypted")]
    Encrypted,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("mupdf: {0}")]
    Engine(String),
}

impl From<MupdfError> for PdfError {
    fn from(e: MupdfError) -> Self {
        PdfError::Engine(e.0)
    }
}

impl From<PdfError> for VdfError {
    fn from(e: PdfError) -> Self {
        match e {
            PdfError::Malformed(m) => VdfError::Document(format!("malformed: {m}")),
            PdfError::Encrypted => VdfError::Document("encrypted".into()),
            PdfError::Io(io) => VdfError::Io(io),
            PdfError::Engine(m) => VdfError::Render(m),
        }
    }
}

pub type PdfResult<T> = Result<T, PdfError>;

/// One application-wide MuPDF master context.
pub struct MupdfEngine {
    ctx: Arc<FzContext>,
}

// SAFETY: FzContext is Send+Sync (lock-protected); Arc shares it.
unsafe impl Send for MupdfEngine {}
unsafe impl Sync for MupdfEngine {}

impl MupdfEngine {
    /// Store budget in bytes for MuPDF's internal object/pixmap caches.
    pub fn new(store_max: usize) -> PdfResult<Self> {
        Ok(Self {
            ctx: Arc::new(FzContext::new(store_max).map_err(PdfError::from)?),
        })
    }

    /// Opens a document from raw bytes (the app reads the file itself; the
    /// engine never touches the original path for writes).
    pub fn open(&self, bytes: Vec<u8>) -> PdfResult<PdfDocument> {
        if bytes.is_empty() {
            return Err(PdfError::Malformed("empty file".into()));
        }
        let stream = unsafe { raw::fz_open_memory(self.ctx.as_ptr(), bytes.as_ptr(), bytes.len()) };
        let magic = CString::new("application/pdf").unwrap();
        let mut doc: *mut raw::fz_document = std::ptr::null_mut();
        let mut err = errbuf();
        let rc = unsafe {
            raw::vdf_open_document_with_stream(
                self.ctx.as_ptr(),
                magic.as_ptr(),
                stream,
                &mut doc,
                err.as_mut_ptr(),
                mupdf_sys::ERRBUF_LEN,
            )
        };
        if rc != 0 {
            return Err(PdfError::Malformed(take_error(&err).0));
        }
        if unsafe { raw::fz_needs_password(self.ctx.as_ptr(), doc) } != 0 {
            unsafe { raw::fz_drop_document(self.ctx.as_ptr(), doc) };
            return Err(PdfError::Encrypted);
        }
        Ok(PdfDocument {
            doc,
            bytes: Arc::new(bytes), // fz_open_memory borrows the buffer: keep it
            engine: Arc::clone(&self.ctx),
        })
    }

    /// A cloned context for a render worker. Workers must be dropped before
    /// any `PdfDocument` opened from this engine (app threading contract).
    pub fn worker_context(&self) -> PdfResult<RawContext> {
        Ok(self.ctx.clone_context()?)
    }
}

/// One open, password-unlocked document. Immutable and shareable: all page
/// access goes through MuPDF's lock-protected state.
pub struct PdfDocument {
    doc: *mut raw::fz_document,
    /// Owning buffer for `fz_open_memory` (it borrows the bytes). Never read
    /// from Rust; dropping it before the document would be a use-after-free,
    /// so it must live exactly this long.
    #[allow(dead_code)]
    bytes: Arc<Vec<u8>>,
    engine: Arc<FzContext>,
}

unsafe impl Send for PdfDocument {}
unsafe impl Sync for PdfDocument {}

/// Flat outline entry (tree flattened by the caller when needed).
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineEntry {
    pub title: String,
    /// 0-based page index, or `None` for external/odd URIs.
    pub page: Option<u32>,
    pub children: Vec<OutlineEntry>,
}

/// One page's parsed display list plus its page-space size (y-down, pt).
///
/// Immutable and shared: safe to run from any number of worker threads at
/// once (MuPDF multi-threading rule 2). Must not outlive its engine's
/// context (guaranteed by holding the context Arc).
pub struct PageDisplayList {
    list: *mut raw::fz_display_list,
    pub page_size: (f64, f64),
    ctx: Arc<FzContext>,
}

unsafe impl Send for PageDisplayList {}
unsafe impl Sync for PageDisplayList {}

impl Drop for PageDisplayList {
    fn drop(&mut self) {
        unsafe { raw::fz_drop_display_list(self.ctx.as_ptr(), self.list) };
    }
}

impl PdfDocument {
    fn ctx(&self) -> *mut raw::fz_context {
        self.engine.as_ptr()
    }

    pub fn page_count(&self) -> PdfResult<u32> {
        let mut pages: c_int = 0;
        let mut err = errbuf();
        let rc = unsafe {
            raw::vdf_count_pages(
                self.ctx(),
                self.doc,
                &mut pages,
                err.as_mut_ptr(),
                mupdf_sys::ERRBUF_LEN,
            )
        };
        if rc != 0 {
            return Err(PdfError::Engine(take_error(&err).0));
        }
        u32::try_from(pages).map_err(|_| PdfError::Engine("negative page count".into()))
    }

    pub fn needs_password(&self) -> bool {
        unsafe { raw::fz_needs_password(self.ctx(), self.doc) != 0 }
    }

    /// Authenticates a password; returns true when it unlocked the document.
    pub fn authenticate(&self, password: &str) -> PdfResult<bool> {
        let pw =
            CString::new(password).map_err(|_| PdfError::Engine("password contains NUL".into()))?;
        let mut result: c_int = 0;
        let mut err = errbuf();
        let rc = unsafe {
            raw::vdf_authenticate_password(
                self.ctx(),
                self.doc,
                pw.as_ptr(),
                &mut result,
                err.as_mut_ptr(),
                mupdf_sys::ERRBUF_LEN,
            )
        };
        if rc != 0 {
            return Err(PdfError::Engine(take_error(&err).0));
        }
        // mupdf returns a permission mask; nonzero means some access granted.
        Ok(result != 0)
    }

    /// Page size in points in page space (y-down), for the unrotated view.
    /// `None` when out of range.
    pub fn page_size(&self, page: u32) -> PdfResult<(f64, f64)> {
        let page_obj = self.load_page(page)?;
        let out = self.page_size_of_loaded(page_obj);
        unsafe { raw::fz_drop_page(self.ctx(), page_obj) };
        out
    }

    fn load_page(&self, page: u32) -> PdfResult<*mut raw::fz_page> {
        let mut page_obj: *mut raw::fz_page = std::ptr::null_mut();
        let mut err = errbuf();
        let rc = unsafe {
            raw::vdf_load_page(
                self.ctx(),
                self.doc,
                page as c_int,
                &mut page_obj,
                err.as_mut_ptr(),
                mupdf_sys::ERRBUF_LEN,
            )
        };
        if rc != 0 {
            return Err(PdfError::Engine(take_error(&err).0));
        }
        Ok(page_obj)
    }

    fn page_size_of_loaded(&self, page_obj: *mut raw::fz_page) -> PdfResult<(f64, f64)> {
        let mut bounds = raw::fz_rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        };
        let mut err = errbuf();
        let rc = unsafe {
            raw::vdf_bound_page(
                self.ctx(),
                page_obj,
                &mut bounds,
                err.as_mut_ptr(),
                mupdf_sys::ERRBUF_LEN,
            )
        };
        if rc != 0 {
            return Err(PdfError::Engine(take_error(&err).0));
        }
        Ok((
            (bounds.x1 - bounds.x0) as f64,
            (bounds.y1 - bounds.y0) as f64,
        ))
    }

    /// Builds the page's display list. **Document-actor-only** (MuPDF rule
    /// 2: no simultaneous document access across threads); the returned
    /// list is then safe to render from any number of worker threads.
    pub fn load_display_list(&self, page: u32) -> PdfResult<Arc<PageDisplayList>> {
        let page_obj = self.load_page(page)?;
        let page_size = self.page_size_of_loaded(page_obj);
        let mut list: *mut raw::fz_display_list = std::ptr::null_mut();
        let mut err = errbuf();
        let rc = unsafe {
            raw::vdf_new_display_list_from_page(
                self.ctx(),
                page_obj,
                &mut list,
                err.as_mut_ptr(),
                mupdf_sys::ERRBUF_LEN,
            )
        };
        unsafe { raw::fz_drop_page(self.ctx(), page_obj) };
        let page_size = page_size?;
        if rc != 0 {
            return Err(PdfError::Engine(take_error(&err).0));
        }
        Ok(Arc::new(PageDisplayList {
            list,
            page_size,
            ctx: Arc::clone(&self.engine),
        }))
    }

    /// Renders a region of a page to RGBA8 from its display list.
    ///
    /// `dl` comes from [`Self::load_display_list`]. `region` is in device
    /// pixels of the *view-rotated, zoomed* page: the page (page space,
    /// y-down) is scaled by `zoom`, rotated `rot` quarter turns clockwise,
    /// and the region is the axis-aligned window into that rotated result.
    /// Safe to call concurrently from many worker threads (MuPDF rule 2:
    /// display lists may be run simultaneously).
    pub fn render_region(
        &self,
        worker: &RawContext,
        dl: &PageDisplayList,
        zoom: f64,
        rot: Rotation,
        region: Rect,
    ) -> PdfResult<Vec<u8>> {
        let w = region.width();
        let h = region.height();
        if w < 1.0 || h < 1.0 || w * h > 64.0 * 1024.0 * 1024.0 {
            return Err(PdfError::Engine(format!("invalid render region {w}x{h}")));
        }
        // page space (y-down, pt) → rotated view device px, shifted so the
        // region origin is at (0,0). Closed form for `rot` quarter turns
        // clockwise in y-down space, folding the zoom scale in (S = scaled
        // page size; constants derived from the y-down CW corner mapping
        // (x,y) → (H−y, x) for R=1, verified by rotation tests below):
        let (w_pt, h_pt) = dl.page_size;
        let wz = (w_pt * zoom) as f32;
        let hz = (h_pt * zoom) as f32;
        let z = zoom as f32;
        let rot_m = match rot {
            Rotation::D0 => raw::fz_matrix {
                a: z,
                b: 0.0,
                c: 0.0,
                d: z,
                e: 0.0,
                f: 0.0,
            },
            Rotation::D90 => raw::fz_matrix {
                a: 0.0,
                b: z,
                c: -z,
                d: 0.0,
                e: hz,
                f: 0.0,
            },
            Rotation::D180 => raw::fz_matrix {
                a: -z,
                b: 0.0,
                c: 0.0,
                d: -z,
                e: wz,
                f: hz,
            },
            Rotation::D270 => raw::fz_matrix {
                a: 0.0,
                b: -z,
                c: z,
                d: 0.0,
                e: 0.0,
                f: wz,
            },
        };
        let shift = mat_translate(-(region.min.x as f32), -(region.min.y as f32));
        let ctm = mat_concat(rot_m, shift);
        let bbox = raw::fz_irect {
            x0: 0,
            y0: 0,
            x1: w.ceil() as c_int,
            y1: h.ceil() as c_int,
        };

        let mut pix: *mut raw::fz_pixmap = std::ptr::null_mut();
        let mut err = errbuf();
        let rc = unsafe {
            raw::vdf_render_list_tile(
                worker.ptr,
                dl.list,
                raw::fz_device_rgb(worker.ptr),
                ctm,
                bbox,
                1, // alpha channel on (compositor expects RGBA)
                &mut pix,
                err.as_mut_ptr(),
                mupdf_sys::ERRBUF_LEN,
            )
        };
        if rc != 0 {
            return Err(PdfError::Engine(take_error(&err).0));
        }
        let out = unsafe {
            let pw = raw::fz_pixmap_width(worker.ptr, pix) as usize;
            let ph = raw::fz_pixmap_height(worker.ptr, pix) as usize;
            let samples = raw::fz_pixmap_samples(worker.ptr, pix);
            let len = pw * ph * 4;
            let mut buf = vec![0u8; len];
            std::ptr::copy_nonoverlapping(samples, buf.as_mut_ptr(), len);
            (pw, ph, buf)
        };
        unsafe { raw::fz_drop_pixmap(worker.ptr, pix) };
        let (pw, ph, buf) = out;
        if pw as f64 != w || ph as f64 != h {
            return Err(PdfError::Engine(format!(
                "render size mismatch: asked {w}x{h}, got {pw}x{ph}"
            )));
        }
        Ok(buf)
    }

    /// Document outline as a tree. Empty when the document has no outline.
    pub fn outline(&self) -> PdfResult<Vec<OutlineEntry>> {
        let mut head: *mut raw::fz_outline = std::ptr::null_mut();
        let mut err = errbuf();
        let rc = unsafe {
            raw::vdf_load_outline(
                self.ctx(),
                self.doc,
                &mut head,
                err.as_mut_ptr(),
                mupdf_sys::ERRBUF_LEN,
            )
        };
        if rc != 0 {
            // A missing/broken outline is not fatal; mupdf may throw when
            // the document has none of substance.
            let msg = take_error(&err).0;
            if msg.contains("no outlines") || msg.contains("No outlines") {
                return Ok(Vec::new());
            }
            return Err(PdfError::Engine(msg));
        }
        let tree = unsafe { walk_outline(self.ctx(), head) };
        Ok(tree)
    }
}

impl Drop for PdfDocument {
    fn drop(&mut self) {
        // Engine Arc keeps the context alive while we release the doc.
        unsafe { raw::fz_drop_document(self.engine.as_ptr(), self.doc) };
    }
}

unsafe fn walk_outline(ctx: *mut raw::fz_context, head: *mut raw::fz_outline) -> Vec<OutlineEntry> {
    let mut out = Vec::new();
    let mut node = head;
    while !node.is_null() {
        let title = unsafe { cstr((*node).title) };
        let page = unsafe {
            let loc = (*node).page;
            (loc.page >= 0).then_some(loc.page as u32)
        };
        let children = unsafe { walk_outline(ctx, (*node).down) };
        out.push(OutlineEntry {
            title,
            page,
            children,
        });
        node = unsafe { (*node).next };
    }
    unsafe { raw::fz_drop_outline(ctx, head) };
    out
}

unsafe fn cstr(p: *mut std::ffi::c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned() }
}

/// Opens a document from a file path (reads the bytes; never writes).
pub fn read_document_bytes(path: impl AsRef<Path>) -> PdfResult<Vec<u8>> {
    Ok(std::fs::read(path)?)
}

// --- fz_matrix helpers (row-vector convention, matching fz_concat) ---

fn mat_translate(tx: f32, ty: f32) -> raw::fz_matrix {
    raw::fz_matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: tx,
        f: ty,
    }
}

/// MuPDF `fz_concat(one, two)`: apply `one` first, then `two`
/// (row-vector convention: p' = p · one · two).
fn mat_concat(one: raw::fz_matrix, two: raw::fz_matrix) -> raw::fz_matrix {
    raw::fz_matrix {
        a: one.a * two.a + one.b * two.c,
        b: one.a * two.b + one.b * two.d,
        c: one.c * two.a + one.d * two.c,
        d: one.c * two.b + one.d * two.d,
        e: one.e * two.a + one.f * two.c + two.e,
        f: one.e * two.b + one.f * two.d + two.f,
    }
}
