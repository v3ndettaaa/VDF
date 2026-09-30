//! Application core state: one engine, one `DocCore` per open document with
//! its loader thread, scheduler, layout, zoom controller, and thumbnail
//! cache (MASTER_PLAN.md §8 threading model).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Sender, unbounded};
use vdf_core::{DocHandle, DocumentId, PageIndex, Rect, Rotation, VdfError, VdfResult};
use vdf_pdf::{MupdfEngine, PdfDocument};
use vdf_render::{
    DocumentLayout, DocumentZoomController, MemoryTransport, PageMode, RenderScheduler,
    SchedulerConfig, ViewportState, compute_layout,
};

/// Messages for the per-document loader thread (all document mutation is
/// serial here: display-list builds, page-size sweeps, thumbnails).
pub enum LoaderMsg {
    SweepSizes,
    LoadDl(u32),
    Thumb(u32, u32),
}

/// (page, width) → RGBA thumbnail pixels.
pub type ThumbMap = Mutex<HashMap<(u32, u32), Arc<Vec<u8>>>>;

/// Layout revision slot.
pub struct LayoutSlot {
    pub rev: u64,
    pub layout: DocumentLayout,
}

/// One open document: everything the viewport talks to.
pub struct DocCore {
    pub handle: DocHandle,
    pub path: String,
    pub engine: Arc<MupdfEngine>,
    pub doc: Arc<PdfDocument>,
    pub page_count: u32,
    pub scheduler: Arc<RenderScheduler>,
    /// Page sizes in pt (y-down); correct values arrive via `SweepSizes`
    /// (before that, page 0's size stands in for all — most PDFs are uniform).
    pub page_sizes: Mutex<Vec<(f64, f64)>>,
    pub sizes_known: AtomicU64,
    pub layout: Mutex<LayoutSlot>,
    pub zoom: Mutex<DocumentZoomController>,
    pub mode: Mutex<PageMode>,
    pub rotation: Mutex<Rotation>,
    pub dpr: Mutex<f64>,
    pub viewport_px: Mutex<(f64, f64)>,
    pub last_viewport: Mutex<Option<ViewportState>>,
    pub loader_tx: Sender<LoaderMsg>,
    /// (page, width) → RGBA
    pub thumbs: ThumbMap,
    /// Consumed by poll: (page, w, h) ready since last poll.
    pub thumb_ready: Mutex<Vec<(u32, u32, u32)>>,
    pub active_page: AtomicU64,
    /// Loader-thread-only worker context for thumbnails.
    thumb_ctx: Mutex<Option<mupdf_sys::RawContext>>,
    layout_rev: AtomicU64,
}

unsafe impl Send for DocCore {}
unsafe impl Sync for DocCore {}

impl DocCore {
    /// Recomputes the layout from current sizes/zoom/mode/rotation/dpr and
    /// bumps its revision.
    pub fn rebuild_layout(&self) {
        let sizes = self.page_sizes.lock().unwrap().clone();
        let layout = compute_layout(
            &sizes,
            *self.mode.lock().unwrap(),
            self.zoom.lock().unwrap().zoom(),
            *self.rotation.lock().unwrap(),
            *self.dpr.lock().unwrap(),
        );
        let mut slot = self.layout.lock().unwrap();
        slot.rev += 1;
        slot.layout = layout;
        drop(slot);
        self.layout_rev.fetch_add(1, Ordering::Relaxed);
        self.retry_viewport();
    }

    /// Re-pushes the last viewport to the scheduler (after a DL appeared).
    pub fn retry_viewport(&self) {
        let vp = self.last_viewport.lock().unwrap().clone();
        if let Some(vp) = vp {
            self.scheduler.update_viewport(vp);
        }
    }

    /// Builds the scheduler viewport snapshot from current state.
    pub fn viewport_state(&self, velocity_y: f64, interacting: bool) -> ViewportState {
        let visible = self.visible_rect();
        let layout = Arc::new(self.current_layout());
        let zoom = self.zoom.lock().unwrap().zoom();
        let rotation = *self.rotation.lock().unwrap();
        ViewportState {
            doc_rev: 1,
            layout,
            visible,
            zoom,
            zoom_step: vdf_core::tile::zoom_step_for(zoom),
            rotation,
            scroll_velocity_y: velocity_y,
            interacting,
        }
    }

    pub fn current_layout(&self) -> DocumentLayout {
        self.layout.lock().unwrap().layout.clone()
    }

    pub fn layout_rev(&self) -> u64 {
        self.layout.lock().unwrap().rev
    }

    /// Visible rect in view space (device px).
    pub fn visible_rect(&self) -> Rect {
        let (sx, sy) = self.zoom.lock().unwrap().scroll();
        let (vw, vh) = *self.viewport_px.lock().unwrap();
        Rect {
            min: vdf_core::Point::new(sx, sy),
            max: vdf_core::Point::new(sx + vw, sy + vh),
        }
    }

    /// Clamps scroll into the document bounds (with a little slack).
    pub fn clamp_scroll(&self) {
        let mut z = self.zoom.lock().unwrap();
        let (vw, vh) = *self.viewport_px.lock().unwrap();
        let layout = self.current_layout();
        let max_x = (layout.doc_size.0 - vw).max(0.0);
        let max_y = (layout.doc_size.1 - vh).max(0.0);
        let (sx, sy) = z.scroll();
        z.set_scroll(sx.clamp(0.0, max_x), sy.clamp(0.0, max_y));
    }

    /// Which page is centered in the viewport.
    pub fn update_active_page(&self) {
        let vis = self.visible_rect();
        let cx = (vis.min.x + vis.max.x) / 2.0;
        let cy = (vis.min.y + vis.max.y) / 2.0;
        if let Some(p) = self.current_layout().page_at(cx, cy) {
            self.active_page.store(p.0 as u64, Ordering::Relaxed);
        }
    }

    /// Scroll target (top of the page, view px) for goto-page.
    pub fn page_scroll_target(&self, page: u32) -> Option<(f64, f64)> {
        let rect = self.current_layout().rect_of(PageIndex(page))?;
        let (_, vh) = *self.viewport_px.lock().unwrap();
        let page_h = rect.height();
        let y = if page_h >= vh {
            rect.min.y
        } else {
            rect.min.y - (vh - page_h) / 2.0
        };
        Some((0.0, y.max(0.0)))
    }

    /// Requests DLs for all pages intersecting `visible` (+ margin) from the
    /// loader thread (serial builds — never blocks input).
    pub fn ensure_display_lists(&self, visible: Rect) {
        let layout = self.current_layout();
        let margin = 1200.0;
        let band = visible.inflated(margin);
        for page in layout.pages_intersecting(band) {
            let loaded = self
                .scheduler
                .display_lists
                .lock()
                .unwrap()
                .contains_key(&page.index.0);
            if !loaded {
                let _ = self.loader_tx.send(LoaderMsg::LoadDl(page.index.0));
            }
        }
    }

    /// Takes the thumbnail-ready list (drained).
    pub fn take_thumb_ready(&self) -> Vec<(u32, u32, u32)> {
        std::mem::take(&mut *self.thumb_ready.lock().unwrap())
    }

    pub fn thumb(&self, page: u32, width: u32) -> Option<Arc<Vec<u8>>> {
        self.thumbs.lock().unwrap().get(&(page, width)).cloned()
    }

    // ---- high-level operations (shared by Tauri commands and the headless
    // e2e test — one code path for both) ----

    /// Applies a viewport update: sizes, dpr, scroll (clamped), DL ensures,
    /// scheduler push.
    #[allow(clippy::too_many_arguments)]
    pub fn viewport(
        &self,
        viewport_w: f64,
        viewport_h: f64,
        dpr: f64,
        scroll_x: f64,
        scroll_y: f64,
        velocity_y: f64,
        interacting: bool,
    ) {
        *self.viewport_px.lock().unwrap() = (viewport_w, viewport_h);
        *self.dpr.lock().unwrap() = dpr;
        self.zoom.lock().unwrap().set_scroll(scroll_x, scroll_y);
        self.clamp_scroll();
        self.update_active_page();
        let visible = self.visible_rect();
        self.ensure_display_lists(visible);
        let vs = self.viewport_state(velocity_y, interacting);
        // Remember the last push so loader-driven retries re-request tiles
        // for pages whose display list was still building.
        *self.last_viewport.lock().unwrap() = Some(vs.clone());
        self.scheduler.update_viewport(vs);
    }

    /// Poll: drain scheduler results; returns ready tile keys (+ counters).
    pub fn poll(&self) -> (Vec<String>, u64, u64) {
        let update = self.scheduler.drain_results();
        let stale = update.stale_dropped;
        let ready: Vec<String> = update
            .ready
            .iter()
            .map(crate::commands::tile_key_str)
            .collect();
        let rendered = self.scheduler.rendered_total();
        (ready, stale, rendered)
    }

    pub fn pan(&self, dx: f64, dy: f64) {
        {
            let mut z = self.zoom.lock().unwrap();
            let (sx, sy) = z.scroll();
            z.set_scroll(sx + dx, sy + dy);
        }
        self.clamp_scroll();
    }

    /// Focal zoom through the central controller (factor or absolute).
    pub fn zoom(&self, factor: f64, focal: (f64, f64), absolute: Option<f64>) -> f64 {
        let vp = *self.viewport_px.lock().unwrap();
        let new_zoom = {
            let mut z = self.zoom.lock().unwrap();
            if let Some(target) = absolute {
                z.set_zoom_focal(target, focal, vp);
            } else {
                let cur = z.zoom();
                z.set_zoom_focal(cur * factor, focal, vp);
            }
            z.zoom()
        };
        self.rebuild_layout();
        self.clamp_scroll();
        new_zoom
    }

    pub fn set_page_mode(&self, mode: PageMode) {
        *self.mode.lock().unwrap() = mode;
        self.rebuild_layout();
        self.clamp_scroll();
    }

    pub fn rotate(&self, quarter_turns: u32) {
        let cur = self.rotation.lock().unwrap().quarter_turns();
        *self.rotation.lock().unwrap() = Rotation::from_quarter_turns(cur + quarter_turns);
        self.rebuild_layout();
        self.clamp_scroll();
    }

    /// Scroll so a page is visible; returns the applied target.
    pub fn goto_page(&self, page: u32) -> VdfResult<(f64, f64)> {
        if page >= self.page_count {
            return Err(VdfError::Document(format!("page {page} out of range")));
        }
        let target = self
            .page_scroll_target(page)
            .ok_or_else(|| VdfError::Document("no layout yet".into()))?;
        self.zoom.lock().unwrap().set_scroll(target.0, target.1);
        self.clamp_scroll();
        self.update_active_page();
        let visible = self.visible_rect();
        self.ensure_display_lists(visible);
        let vs = self.viewport_state(0.0, false);
        *self.last_viewport.lock().unwrap() = Some(vs.clone());
        self.scheduler.update_viewport(vs);
        Ok(target)
    }

    /// Fit-width zoom factor for the current viewport and widest page.
    pub fn fit_width(&self) -> f64 {
        let (vw, _) = *self.viewport_px.lock().unwrap();
        let sizes = self.page_sizes.lock().unwrap();
        let rot = *self.rotation.lock().unwrap();
        let max_w_pt = sizes
            .iter()
            .map(|&(w, h)| match rot {
                Rotation::D0 | Rotation::D180 => w,
                _ => h,
            })
            .fold(0.0f64, f64::max);
        let dpr = *self.dpr.lock().unwrap();
        (vw / (max_w_pt * dpr)).clamp(vdf_render::ZOOM_MIN, vdf_render::ZOOM_MAX)
    }

    pub fn request_thumbnails(&self, pages: &[u32], width: u32) {
        for &page in pages {
            let _ = self.loader_tx.send(LoaderMsg::Thumb(page, width));
        }
    }

    pub fn outline(&self) -> Vec<vdf_pdf::OutlineEntry> {
        self.doc.outline().unwrap_or_default()
    }
}

/// The application-wide PDF engine (one master MuPDF context) + open docs.
pub struct AppCore {
    pub engine: Arc<MupdfEngine>,
    pub docs: Mutex<HashMap<u64, Arc<DocCore>>>,
    /// Loader-channel senders, held here (not only in DocCore) so closing
    /// a document can close the channel: the loader holds the core weakly,
    /// so dropping both senders ends the loader and releases the DocCore.
    loaders: Mutex<HashMap<u64, Sender<LoaderMsg>>>,
    next_doc: AtomicU64,
}

impl AppCore {
    pub fn new() -> VdfResult<Self> {
        let total = vdf_render::cache::total_ram_bytes();
        let budgets = vdf_render::MemoryBudgets::from_total_ram(total);
        Ok(Self {
            // The MuPDF store shares pressure with the tile cache; give it
            // roughly the tile budget (validated by M7 profiling).
            engine: Arc::new(MupdfEngine::new(budgets.tile_cache_bytes)?),
            docs: Mutex::new(HashMap::new()),
            loaders: Mutex::new(HashMap::new()),
            next_doc: AtomicU64::new(1),
        })
    }

    pub fn open_document(&self, path: &str, bytes: Vec<u8>) -> VdfResult<Arc<DocCore>> {
        let id = self.next_doc.fetch_add(1, Ordering::Relaxed);
        let doc = Arc::new(self.engine.open(bytes)?);
        let page_count = doc.page_count()?;
        let first = doc.page_size(0).unwrap_or((612.0, 792.0));

        let transport = Arc::new(MemoryTransport::new());
        let renderer = Arc::new(vdf_render::MupdfTileRenderer::new(
            Arc::clone(&self.engine),
            Arc::clone(&doc),
        ));
        let scheduler = Arc::new(RenderScheduler::new(
            DocumentId(id),
            renderer,
            transport,
            SchedulerConfig::default(),
        ));

        let (loader_tx, loader_rx) = unbounded::<LoaderMsg>();
        let core = Arc::new(DocCore {
            handle: DocumentId(id),
            path: path.to_string(),
            engine: Arc::clone(&self.engine),
            doc,
            page_count,
            scheduler,
            page_sizes: Mutex::new(vec![first; page_count as usize]),
            sizes_known: AtomicU64::new(0),
            layout: Mutex::new(LayoutSlot {
                rev: 1,
                layout: compute_layout(&[first], PageMode::Continuous, 1.0, Rotation::D0, 1.0),
            }),
            zoom: Mutex::new(DocumentZoomController::new()),
            mode: Mutex::new(PageMode::Continuous),
            rotation: Mutex::new(Rotation::D0),
            dpr: Mutex::new(1.0),
            viewport_px: Mutex::new((1280.0, 800.0)),
            last_viewport: Mutex::new(None),
            loader_tx: loader_tx.clone(),
            thumbs: Mutex::new(HashMap::new()),
            thumb_ready: Mutex::new(Vec::new()),
            active_page: AtomicU64::new(0),
            thumb_ctx: Mutex::new(None),
            layout_rev: AtomicU64::new(1),
        });

        let core2 = Arc::clone(&core);
        std::thread::Builder::new()
            .name(format!("vdf-loader-{id}"))
            .spawn(move || loader_loop(Arc::downgrade(&core2), loader_rx))?;
        self.loaders.lock().unwrap().insert(id, loader_tx);
        self.docs.lock().unwrap().insert(id, Arc::clone(&core));
        let _ = core.loader_tx.send(LoaderMsg::SweepSizes);
        Ok(core)
    }

    pub fn close_document(&self, id: u64) {
        let core = self.docs.lock().unwrap().remove(&id);
        self.loaders.lock().unwrap().remove(&id);
        if let Some(core) = core {
            core.scheduler.shutdown_shared();
        }
        // Loader exits: channel closed (both senders gone), then the weak
        // upgrade fails or recv errors, releasing the DocCore.
    }
}

fn loader_loop(core: std::sync::Weak<DocCore>, rx: crossbeam_channel::Receiver<LoaderMsg>) {
    while let Ok(msg) = rx.recv() {
        let Some(core) = core.upgrade() else { break };
        match msg {
            LoaderMsg::SweepSizes => {
                // Every page's real size, chunked so scrolling during the
                // sweep already lands correctly.
                let count = core.page_count as usize;
                for i in 0..count {
                    if let Ok(size) = core.doc.page_size(i as u32) {
                        core.page_sizes.lock().unwrap()[i] = size;
                    }
                    core.sizes_known.fetch_add(1, Ordering::Relaxed);
                    if i % 32 == 31 || i + 1 == count {
                        core.rebuild_layout();
                    }
                }
            }
            LoaderMsg::LoadDl(page) => {
                let already = core
                    .scheduler
                    .display_lists
                    .lock()
                    .unwrap()
                    .contains_key(&page);
                if !already {
                    match core.doc.load_display_list(page) {
                        Ok(dl) => {
                            core.scheduler
                                .display_lists
                                .lock()
                                .unwrap()
                                .insert(page, dl);
                        }
                        Err(_e) => {}
                    }
                }
                core.retry_viewport();
            }
            LoaderMsg::Thumb(page, width) => {
                let key = (page, width);
                if core.thumbs.lock().unwrap().contains_key(&key) {
                    continue;
                }
                // DL if present, else build one (loader thread: serial).
                let dl = {
                    let dls = core.scheduler.display_lists.lock().unwrap();
                    dls.get(&page).cloned()
                };
                let dl = match dl {
                    Some(dl) => dl,
                    None => match core.doc.load_display_list(page) {
                        Ok(dl) => {
                            core.scheduler
                                .display_lists
                                .lock()
                                .unwrap()
                                .insert(page, dl.clone());
                            dl
                        }
                        Err(_) => continue,
                    },
                };
                if let Ok(((w, h), buf)) = render_thumb(&core, &dl, width) {
                    // wire format: [w u32 LE][h u32 LE][RGBA...]
                    let mut framed = Vec::with_capacity(8 + buf.len());
                    framed.extend_from_slice(&w.to_le_bytes());
                    framed.extend_from_slice(&h.to_le_bytes());
                    framed.extend_from_slice(&buf);
                    core.thumbs.lock().unwrap().insert(key, Arc::new(framed));
                    core.thumb_ready.lock().unwrap().push((page, w, h));
                }
            }
        }
    }
}

fn render_thumb(
    core: &DocCore,
    dl: &Arc<vdf_pdf::PageDisplayList>,
    width: u32,
) -> VdfResult<((u32, u32), Vec<u8>)> {
    let (w_pt, h_pt) = dl.page_size;
    let w = width;
    let h = ((h_pt / w_pt) * width as f64).round().max(1.0) as u32;
    // One worker context per loader thread, created lazily.
    let ctx = {
        let mut guard = core.thumb_ctx.lock().unwrap();
        if guard.is_none() {
            *guard = Some(core.engine.worker_context()?);
        }
        let raw_ptr = guard.as_ref().expect("just set").ptr;
        // The TLS-like owner lives in `thumb_ctx`; the temporary handle here
        // must not drop the context.
        std::mem::ManuallyDrop::new(mupdf_sys::RawContext { ptr: raw_ptr })
    };
    let buf = core.doc.render_region(
        &ctx,
        dl,
        w as f64 / w_pt,
        *core.rotation.lock().unwrap(),
        Rect::from_size(w as f64, h as f64),
    )?;
    Ok(((w, h), buf))
}
