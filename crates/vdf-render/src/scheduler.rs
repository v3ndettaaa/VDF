//! Render scheduler: turns viewport state into prioritized tile jobs,
//! renders them on a worker pool, and feeds generation-checked results
//! into the cache and the transport (MASTER_PLAN.md §8, §10).
//!
//! Invariants enforced here:
//! - stale results (older generation for a slot) are dropped, never
//!   published — the no-overwrite rule
//! - visible tiles are pinned against eviction
//! - jobs whose display list is not loaded yet are skipped and retried on
//!   the next viewport update (the document actor loads them serially)

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, Sender, bounded};

use vdf_core::tile::{RenderGeneration, TileKey};
use vdf_core::{DocHandle, Rect, Rotation, VdfResult};
use vdf_pdf::PageDisplayList;

use crate::cache::{MemoryBudgets, TileCache};
use crate::layout::DocumentLayout;
use crate::renderer::{TileJob, TileRenderer};
use crate::transport::{FinishedTile, RenderTransport};

/// Tile target in device px (a hypothesis validated by profiling, §10).
pub const TILE_PX: u32 = 256;

/// Scheduler tuning.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub workers: usize,
    /// Tiles enqueued per viewport update (burst limiter).
    pub max_enqueue_per_update: usize,
    /// Extra pages beyond visible to prefetch (low-res aware in M7).
    pub near_band_px: f64,
    pub budgets: MemoryBudgets,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        let workers = std::thread::available_parallelism()
            .map(|n| n.get().clamp(2, 6))
            .unwrap_or(4);
        Self {
            workers,
            max_enqueue_per_update: 96,
            near_band_px: 1600.0,
            budgets: MemoryBudgets::from_total_ram(crate::cache::total_ram_bytes()),
        }
    }
}

/// Viewport snapshot pushed by the app on every scroll/zoom/resize.
#[derive(Debug, Clone)]
pub struct ViewportState {
    pub doc_rev: u64,
    pub layout: Arc<DocumentLayout>,
    /// Visible rect in view space (device px).
    pub visible: Rect,
    pub zoom: f64,
    pub zoom_step: u16,
    pub rotation: Rotation,
    /// Positive = scrolling down (prefetch below).
    pub scroll_velocity_y: f64,
    /// True while a gesture (pan/zoom/pinch) is in progress.
    pub interacting: bool,
}

/// What one `drain_results` call produced.
#[derive(Debug, Default)]
pub struct SchedulerUpdate {
    /// Tiles that became ready (generation-checked, in cache + transport).
    pub ready: Vec<TileKey>,
    pub stale_dropped: u64,
    pub failed: u64,
}

struct Job {
    key: TileKey,
    generation: RenderGeneration,
    doc_rev: u64,
    zoom: f64,
    region: Rect,
    width: u32,
    height: u32,
    display_list: Arc<PageDisplayList>,
}

struct Outcome {
    key: TileKey,
    generation: RenderGeneration,
    doc_rev: u64,
    result: VdfResult<(u32, u32, Vec<u8>)>,
}

struct SchedState {
    viewport: Option<ViewportState>,
    /// Slot generation per key (bumped whenever a new request is issued).
    slot_gen: HashMap<TileKey, RenderGeneration>,
    /// Jobs in flight: key → generation they will satisfy.
    pending: HashMap<TileKey, RenderGeneration>,
    cache: TileCache,
    transport: Arc<dyn RenderTransport>,
    doc_handle: DocHandle,
    stale_dropped: u64,
    failed: u64,
    rendered_total: u64,
}

impl SchedState {}

/// The tile scheduler for one open document.
pub struct RenderScheduler {
    state: Mutex<SchedState>,
    /// Closed to stop the workers; `Mutex`-wrapped so shared shutdown can
    /// drop it (all senders gone ⇒ workers see a closed channel).
    job_tx: Mutex<Option<Sender<Job>>>,
    result_rx: Receiver<Outcome>,
    workers: Mutex<Vec<std::thread::JoinHandle<()>>>,
    /// Display lists shared with the document actor (it loads serially).
    pub display_lists: Arc<Mutex<HashMap<u32, Arc<PageDisplayList>>>>,
    stats_rendered: Arc<AtomicU64>,
    config: SchedulerConfig,
}

impl RenderScheduler {
    /// Spawns the worker pool. `renderer` is typically
    /// [`crate::renderer::MupdfTileRenderer`]; tests inject fakes.
    pub fn new(
        doc_handle: DocHandle,
        renderer: Arc<dyn TileRenderer>,
        transport: Arc<dyn RenderTransport>,
        config: SchedulerConfig,
    ) -> Self {
        let (job_tx, job_rx) = bounded::<Job>(256);
        let (result_tx, result_rx) = bounded::<Outcome>(1024);
        let mut workers = Vec::with_capacity(config.workers);
        for _ in 0..config.workers {
            let job_rx = job_rx.clone();
            let result_tx = result_tx.clone();
            let renderer = Arc::clone(&renderer);
            workers.push(std::thread::spawn(move || {
                loop {
                    let Ok(job) = job_rx.recv() else { return };
                    let result = renderer.render(&TileJob {
                        doc_rev: job.doc_rev,
                        key: job.key,
                        zoom: job.zoom,
                        region: job.region,
                        width: job.width,
                        height: job.height,
                        display_list: Arc::clone(&job.display_list),
                    });
                    let _ = result_tx.send(Outcome {
                        key: job.key,
                        generation: job.generation,
                        doc_rev: job.doc_rev,
                        result: result.map(|px| (job.width, job.height, px)),
                    });
                }
            }));
        }

        Self {
            state: Mutex::new(SchedState {
                viewport: None,
                slot_gen: HashMap::new(),
                pending: HashMap::new(),
                cache: TileCache::new(config.budgets.tile_cache_bytes),
                transport,
                doc_handle,
                stale_dropped: 0,
                failed: 0,
                rendered_total: 0,
            }),
            job_tx: Mutex::new(Some(job_tx)),
            result_rx,
            workers: Mutex::new(workers),
            display_lists: Arc::new(Mutex::new(HashMap::new())),
            stats_rendered: Arc::new(AtomicU64::new(0)),
            config,
        }
    }

    pub fn cache_stats(&self) -> (usize, usize, usize) {
        let st = self.state.lock().unwrap();
        (st.cache.len(), st.cache.used(), st.cache.budget())
    }

    /// Cached pixels for a tile (for the transport protocol handler).
    /// Marks the entry recently used.
    pub fn tile_pixels(&self, key: &TileKey) -> Option<(u32, u32, Arc<Vec<u8>>)> {
        let mut st = self.state.lock().unwrap();
        st.cache.touch(key);
        st.cache
            .get(key)
            .map(|e| (e.width, e.height, Arc::clone(&e.pixels)))
    }

    pub fn stale_dropped_total(&self) -> u64 {
        self.state.lock().unwrap().stale_dropped
    }

    pub fn rendered_total(&self) -> u64 {
        self.stats_rendered.load(Ordering::Relaxed)
    }

    /// Current generation retained for a slot (cache or pending).
    pub fn slot_generation(&self, key: &TileKey) -> Option<RenderGeneration> {
        let st = self.state.lock().unwrap();
        st.cache
            .get(key)
            .map(|e| e.generation)
            .or_else(|| st.pending.get(key).copied())
    }

    /// Pushes a new viewport snapshot and (re)prioritizes tile work.
    pub fn update_viewport(&self, viewport: ViewportState) {
        let mut wanted: Vec<(i64, TileKey, Rect, u32, u32)> = Vec::new();
        {
            // expanded band: visible + near margin (+ prefetch in scroll dir)
            let mut band = viewport.visible.inflated(self.config.near_band_px);
            if viewport.scroll_velocity_y > 1.0 {
                band.max.y += self.config.near_band_px * 2.0;
            } else if viewport.scroll_velocity_y < -1.0 {
                band.min.y -= self.config.near_band_px * 2.0;
            }

            for page in viewport.layout.pages_intersecting(band) {
                let pr = page.rect;
                let grid_w = (pr.width() / TILE_PX as f64).ceil() as u32;
                let grid_h = (pr.height() / TILE_PX as f64).ceil() as u32;
                for ty in 0..grid_h {
                    for tx in 0..grid_w {
                        let tw = (TILE_PX as f64).min(pr.width() - tx as f64 * TILE_PX as f64);
                        let th = (TILE_PX as f64).min(pr.height() - ty as f64 * TILE_PX as f64);
                        if tw <= 0.0 || th <= 0.0 {
                            continue;
                        }
                        // page-local region (render_region takes page-local px)
                        let region = Rect {
                            min: vdf_core::Point::new(
                                tx as f64 * TILE_PX as f64,
                                ty as f64 * TILE_PX as f64,
                            ),
                            max: vdf_core::Point::new(
                                tx as f64 * TILE_PX as f64 + tw,
                                ty as f64 * TILE_PX as f64 + th,
                            ),
                        };
                        // view-space tile rect for zone/priority math
                        let view_tile = Rect {
                            min: vdf_core::Point::new(
                                pr.min.x + region.min.x,
                                pr.min.y + region.min.y,
                            ),
                            max: vdf_core::Point::new(
                                pr.min.x + region.max.x,
                                pr.min.y + region.max.y,
                            ),
                        };
                        if !view_tile.intersects(band) {
                            continue;
                        }
                        let key =
                            TileKey::new(page.index, viewport.zoom_step, viewport.rotation, tx, ty);
                        // priority: visible first, then distance to viewport center,
                        // with a bonus for tiles ahead of the scroll direction
                        let center = view_tile.center();
                        let vcx = (viewport.visible.min.x + viewport.visible.max.x) / 2.0;
                        let vcy = (viewport.visible.min.y + viewport.visible.max.y) / 2.0;
                        let dist = ((center.x - vcx).powi(2) + (center.y - vcy).powi(2)).sqrt();
                        let ahead = if viewport.scroll_velocity_y >= 0.0 {
                            center.y > vcy
                        } else {
                            center.y < vcy
                        };
                        let zone = if viewport.visible.intersects(view_tile) {
                            0
                        } else {
                            1
                        };
                        let priority = ((zone as f64 * 100_000.0) + dist
                            - if ahead { 5_000.0 } else { 0.0 })
                            as i64;
                        wanted.push((priority, key, region, tw.ceil() as u32, th.ceil() as u32));
                    }
                }
            }
            wanted.sort_by_key(|(p, ..)| *p);
        }
        let mut st = self.state.lock().unwrap();
        st.viewport = Some(viewport);
        let mut enqueued = 0usize;
        for (_, key, region, tw, th) in wanted {
            if enqueued >= self.config.max_enqueue_per_update {
                break;
            }
            if st.cache.get(&key).is_some() || st.pending.contains_key(&key) {
                continue;
            }
            let doc_rev = match &st.viewport {
                Some(vp) => vp.doc_rev,
                None => unreachable!(),
            };
            let Some(dl) = self.display_lists.lock().unwrap().get(&key.page.0).cloned() else {
                continue; // dl not loaded yet; next update retries
            };
            let generation = st
                .slot_gen
                .get(&key)
                .copied()
                .unwrap_or(RenderGeneration(0))
                .next();
            st.slot_gen.insert(key, generation);
            st.pending.insert(key, generation);
            let job = Job {
                key,
                generation,
                doc_rev,
                zoom: st.viewport.as_ref().map(|v| v.zoom).unwrap_or(1.0),
                region,
                width: tw,
                height: th,
                display_list: dl,
            };
            let job_tx = self.job_tx.lock().unwrap();
            let Some(tx) = job_tx.as_ref() else {
                // scheduler shut down mid-update
                st.pending.remove(&key);
                continue;
            };
            if tx.try_send(job).is_err() {
                st.pending.remove(&key);
                continue;
            }
            enqueued += 1;
        }
    }

    /// Drains finished renders, applies generation checks, feeds cache +
    /// transport. Returns what became ready for the compositor.
    pub fn drain_results(&self) -> SchedulerUpdate {
        let mut update = SchedulerUpdate::default();
        let mut st = self.state.lock().unwrap();
        while let Some(outcome) = try_next(&self.result_rx) {
            let current_gen = st.slot_gen.get(&outcome.key).copied();
            st.pending.remove(&outcome.key);
            let is_current = matches!(current_gen, Some(g) if g >= outcome.generation)
                && outcome.doc_rev == st.viewport.as_ref().map(|v| v.doc_rev).unwrap_or(u64::MAX);
            match outcome.result {
                Ok((w, h, px)) if is_current => {
                    // snapshot of the visible band for pin decisions
                    let visible = st.viewport.as_ref().map(|v| v.visible);
                    let layout = st.viewport.as_ref().map(|v| Arc::clone(&v.layout));
                    let pinned = move |key: &vdf_core::TileKey| {
                        let (Some(visible), Some(layout)) = (visible, &layout) else {
                            return false;
                        };
                        let Some(pr) = layout.rect_of(vdf_core::PageIndex(key.page.0)) else {
                            return false;
                        };
                        let tx = key.x as f64 * TILE_PX as f64;
                        let ty = key.y as f64 * TILE_PX as f64;
                        let tw = (TILE_PX as f64).min(pr.width() - tx).max(0.0);
                        let th = (TILE_PX as f64).min(pr.height() - ty).max(0.0);
                        if tw <= 0.0 || th <= 0.0 {
                            return false;
                        }
                        Rect {
                            min: vdf_core::Point::new(pr.min.x + tx, pr.min.y + ty),
                            max: vdf_core::Point::new(pr.min.x + tx + tw, pr.min.y + ty + th),
                        }
                        .intersects(visible)
                    };
                    if st.cache.insert(
                        outcome.key,
                        outcome.generation,
                        w,
                        h,
                        std::sync::Arc::new(px.clone()),
                        &pinned,
                    ) {
                        let _ = st.transport.publish_tile(FinishedTile {
                            doc: st.doc_handle,
                            key: outcome.key,
                            generation: outcome.generation,
                            width: w,
                            height: h,
                            rgba: std::sync::Arc::new(px),
                        });
                        update.ready.push(outcome.key);
                        st.rendered_total += 1;
                        self.stats_rendered.fetch_add(1, Ordering::Relaxed);
                    } else {
                        st.stale_dropped += 1;
                        update.stale_dropped += 1;
                    }
                }
                Ok(_) => {
                    st.stale_dropped += 1;
                    update.stale_dropped += 1;
                }
                Err(_) => {
                    st.failed += 1;
                    update.failed += 1;
                }
            }
        }
        update
    }

    /// Drops cached tiles for a document revision change (M2+ edits).
    pub fn invalidate(&self) {
        let mut st = self.state.lock().unwrap();
        st.cache.invalidate(&|_| true);
        st.slot_gen.clear();
        let _ = st.transport.invalidate(st.doc_handle, 0);
    }

    /// Shuts the worker pool down (drop workers before the MuPDF engine).
    pub fn shutdown(mut self) {
        *self.job_tx.lock().unwrap() = None; // closes the channel: workers exit
        for w in self.workers.get_mut().unwrap().drain(..) {
            let _ = w.join();
        }
    }

    /// Shutdown through shared ownership (`Arc<RenderScheduler>`): closes
    /// the job channel and joins the workers; the scheduler object stays
    /// alive but its pool is gone (safe — no further jobs are accepted).
    pub fn shutdown_shared(&self) {
        *self.job_tx.lock().unwrap() = None; // closes the channel: workers exit
        let handles = {
            let mut w = self.workers.lock().unwrap();
            std::mem::take(&mut *w)
        };
        for h in handles {
            let _ = h.join();
        }
    }
}

/// Non-blocking recv helper for the drain loop.
fn try_next(rx: &Receiver<Outcome>) -> Option<Outcome> {
    rx.try_recv().ok()
}

const _: fn() = || {
    // type-level assertions
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RenderScheduler>();
};
