//! vdf-bench — headless scenario harness (MASTER_PLAN.md §13).
//!
//! Scenarios drive the real core crates without a GUI. M1 adds the viewer
//! baselines: open latency, distant-page render, and scheduler throughput
//! with the fake renderer (MuPDF rendering numbers come from the vdf-pdf
//! behavior tests and are echoed here for the record).
//!
//! Gates activate once `bench/baselines.json` exists with real-hardware
//! numbers; until then every run prints `gated: false`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use vdf_core::{DocumentId, Point, Rect, Rotation, VdfResult};
use vdf_pdf::MupdfEngine;
use vdf_render::{
    FakeTileRenderer, MemoryBudgets, MemoryTransport, PageMode, RenderScheduler, SchedulerConfig,
    ViewportState, compute_layout,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Pass,
    Warn,
    Fail,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Pass => "PASS",
            Status::Warn => "WARN",
            Status::Fail => "FAIL",
        }
    }
}

const SCENARIOS: &[&str] = &["smoke", "m1"];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--list") {
        for s in SCENARIOS {
            println!("{s}");
        }
        return;
    }
    let scenario = args
        .iter()
        .position(|a| a == "--scenario")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "smoke".to_string());

    match scenario.as_str() {
        "smoke" => run_smoke(),
        "m1" => run_m1(),
        other => {
            eprintln!("unknown scenario {other:?}; available: {SCENARIOS:?}");
            std::process::exit(2);
        }
    }
}

fn emit(json: serde_json::Value, fail: bool) {
    println!("{json}");
    if fail {
        std::process::exit(1);
    }
}

/// Document-model churn (M0 baseline).
fn run_smoke() {
    use vdf_core::Rect as R;
    use vdf_document::{AddObject, Document, History, SetObjectOpacity};
    let t0 = Instant::now();

    let mut doc = Document::new("bench");
    let page = doc.add_page((612.0, 792.0));
    let mut hist = History::new(10_000);

    let mut ids = Vec::new();
    for i in 0..200 {
        let obj = doc.make_envelope(
            page,
            R::from_center_size((i % 20) as f64 * 10.0, (i / 20) as f64 * 10.0, 8.0, 8.0),
        );
        ids.push(obj.id);
        hist.apply(Box::new(AddObject::new(obj)), &mut doc).unwrap();
    }
    for (i, &id) in ids.iter().enumerate() {
        hist.apply(
            Box::new(SetObjectOpacity::new(id, (i % 10) as f32 / 10.0)),
            &mut doc,
        )
        .unwrap();
    }

    let undo_depth = {
        let mut n = 0;
        while hist.undo(&mut doc).unwrap() {
            n += 1;
        }
        n
    };
    let pristine_objects = doc.object_count();
    while hist.redo(&mut doc).unwrap() {}
    let restored_objects = doc.object_count();

    let elapsed = t0.elapsed();
    let status = if pristine_objects != 0 || restored_objects != 200 || hist.can_redo() {
        Status::Fail
    } else if elapsed.as_millis() > 500 {
        Status::Warn
    } else {
        Status::Pass
    };

    emit(
        serde_json::json!({
            "scenario": "smoke",
            "duration_ms": elapsed.as_secs_f64() * 1000.0,
            "objects": restored_objects,
            "undo_depth": undo_depth,
            "status": status.as_str(),
            "gated": false,
            "note": "baseline-establishing run; gates activate once baselines exist",
        }),
        status == Status::Fail,
    );
}

#[path = "../../vdf-pdf/tests/support/mod.rs"]
mod fixture;
use fixture::make_pdf;

fn scheduler_of(engine: &Arc<MupdfEngine>, doc: Arc<vdf_pdf::PdfDocument>) -> Arc<RenderScheduler> {
    let transport = Arc::new(MemoryTransport::new());
    let renderer = Arc::new(vdf_render::MupdfTileRenderer::new(
        Arc::clone(engine),
        Arc::clone(&doc),
    ));
    Arc::new(RenderScheduler::new(
        DocumentId(1),
        renderer,
        transport,
        SchedulerConfig::default(),
    ))
}

fn drain_quiescent(sched: &RenderScheduler) -> usize {
    let mut ready = 0;
    let mut quiet = 0;
    let mut last = sched.rendered_total();
    for _ in 0..4000 {
        let update = sched.drain_results();
        ready += update.ready.len();
        let total = sched.rendered_total();
        if update.ready.is_empty() && total == last {
            quiet += 1;
            if quiet >= 10 {
                break;
            }
        } else {
            quiet = 0;
        }
        last = total;
        std::thread::sleep(Duration::from_millis(1));
    }
    ready
}

/// M1 viewer baselines: open 100/1000-page PDFs, first-viewport readiness,
/// distant-page render, fast-jump scroll, zoom ladder re-layout.
fn run_m1() {
    let engine = Arc::new(MupdfEngine::new(256 << 20).expect("engine"));
    let mut results = serde_json::Map::new();
    let mut fail = false;

    // --- open 100-page ---
    let bytes100 = make_pdf(100);
    let t = Instant::now();
    let doc100 = Arc::new(engine.open(bytes100).expect("open 100"));
    let pages100 = doc100.page_count().unwrap();
    let open100_ms = t.elapsed().as_secs_f64() * 1000.0;
    results.insert("open100_ms".into(), serde_json::json!(open100_ms));
    results.insert("open100_pages".into(), serde_json::json!(pages100));

    // --- first useful viewport (page 0 dl + first tiles) ---
    let t = Instant::now();
    let dl0 = doc100.load_display_list(0).expect("dl page 0");
    let dl0_ms = t.elapsed().as_secs_f64() * 1000.0;
    results.insert("first_dl_ms".into(), serde_json::json!(dl0_ms));

    let sched = scheduler_of(&engine, doc100.clone());
    sched
        .display_lists
        .lock()
        .unwrap()
        .insert(0, Arc::clone(&dl0));
    let layout = Arc::new(compute_layout(
        &vec![(612.0, 792.0); 100],
        PageMode::Continuous,
        1.0,
        Rotation::D0,
        1.0,
    ));
    let t = Instant::now();
    sched.update_viewport(ViewportState {
        doc_rev: 1,
        layout: Arc::clone(&layout),
        visible: Rect {
            min: Point::new(0.0, 0.0),
            max: Point::new(1200.0, 900.0),
        },
        zoom: 1.0,
        zoom_step: vdf_core::tile::zoom_step_for(1.0),
        rotation: Rotation::D0,
        scroll_velocity_y: 0.0,
        interacting: false,
    });
    let ready = drain_quiescent(&sched);
    let first_viewport_ms = t.elapsed().as_secs_f64() * 1000.0;
    results.insert(
        "first_viewport_ms".into(),
        serde_json::json!(first_viewport_ms),
    );
    results.insert("first_viewport_tiles".into(), serde_json::json!(ready));
    if ready == 0 {
        fail = true;
    }
    sched.shutdown_shared();

    // --- open 1000-page ---
    let bytes1000 = make_pdf(1000);
    let t = Instant::now();
    let doc1000 = Arc::new(engine.open(bytes1000).expect("open 1000"));
    let open1000_ms = t.elapsed().as_secs_f64() * 1000.0;
    results.insert("open1000_ms".into(), serde_json::json!(open1000_ms));

    // --- distant page render (no full-document parse) ---
    let t = Instant::now();
    let dl999 = doc1000.load_display_list(999).expect("dl 999");
    let worker = engine.worker_context().expect("worker ctx");
    let img = doc1000
        .render_region(
            &worker,
            &dl999,
            1.0,
            Rotation::D0,
            Rect::from_size(256.0, 256.0),
        )
        .expect("distant render");
    let distant_ms = t.elapsed().as_secs_f64() * 1000.0;
    results.insert("distant_tile_ms".into(), serde_json::json!(distant_ms));
    results.insert("distant_tile_bytes".into(), serde_json::json!(img.len()));
    drop(worker);
    drop(dl999);

    // --- fast scroll jump: scheduler sweep 1 → 100 → 500 → 900 with the
    // fake renderer (measures scheduling, not rasterization) ---
    let fake = Arc::new(FakeTileRenderer::new());
    let transport = Arc::new(MemoryTransport::new());
    let transport_stats = Arc::clone(&transport);
    let sched = Arc::new(RenderScheduler::new(
        DocumentId(2),
        fake,
        transport,
        SchedulerConfig {
            workers: 4,
            max_enqueue_per_update: 128,
            near_band_px: 1600.0,
            budgets: MemoryBudgets {
                tile_cache_bytes: 64 << 20,
                thumbnail_cache_bytes: 1 << 20,
            },
        },
    ));
    let _ = std::fs::write("/dev/null", b""); // no-op for platform symmetry
    for page in 0..1000u32 {
        sched.display_lists.lock().unwrap().insert(page, null_dl());
    }
    let jumps = [1usize, 100, 500, 900];
    let t = Instant::now();
    for &jump in &jumps {
        let y = (jump as f64) * 842.0;
        sched.update_viewport(ViewportState {
            doc_rev: 1,
            layout: Arc::clone(&layout),
            visible: Rect {
                min: Point::new(0.0, y),
                max: Point::new(1200.0, y + 900.0),
            },
            zoom: 1.0,
            zoom_step: vdf_core::tile::zoom_step_for(1.0),
            rotation: Rotation::D0,
            scroll_velocity_y: 60.0,
            interacting: false,
        });
    }
    let ready = drain_quiescent(&sched);
    let scroll_ms = t.elapsed().as_secs_f64() * 1000.0;
    results.insert("scroll_jump_ms".into(), serde_json::json!(scroll_ms));
    results.insert("scroll_jump_tiles".into(), serde_json::json!(ready));
    results.insert(
        "scroll_transport_published".to_string(),
        serde_json::json!(transport_stats.stats().published),
    );
    sched.shutdown_shared();

    // --- zoom ladder re-layout (100→150→200→300→500→100) ---
    let sizes = vec![(612.0, 792.0); 100];
    let t = Instant::now();
    for z in [1.0, 1.5, 2.0, 3.0, 5.0, 1.0] {
        let _layout = compute_layout(&sizes, PageMode::Continuous, z, Rotation::D0, 1.0);
        let _step = vdf_core::tile::zoom_step_for(z);
    }
    let zoom_ms = t.elapsed().as_secs_f64() * 1000.0;
    results.insert("zoom_ladder_relayout_ms".into(), serde_json::json!(zoom_ms));

    let status = if fail { Status::Fail } else { Status::Pass };
    emit(
        serde_json::json!({
            "scenario": "m1",
            "status": status.as_str(),
            "gated": false,
            "note": "viewer baselines; gates activate once bench/baselines.json exists",
            "metrics": results,
        }),
        fail,
    );
}

/// A tiny placeholder display list is not constructible without MuPDF — the
/// fake renderer never touches it, but TileJob requires the type. Reuse one
/// page-0 list from a minimal doc for every slot.
fn null_dl() -> Arc<vdf_pdf::PageDisplayList> {
    use std::sync::OnceLock;
    static DL: OnceLock<Arc<vdf_pdf::PageDisplayList>> = OnceLock::new();
    DL.get_or_init(|| {
        let engine = MupdfEngine::new(16 << 20).expect("engine");
        let doc = engine.open(make_pdf(1)).expect("open");
        doc.load_display_list(0).expect("dl")
    })
    .clone()
}

/// keep VdfResult referenced on stub paths
#[allow(dead_code)]
fn _t(_: VdfResult<()>) {}
