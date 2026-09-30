//! vdf-pdf behavior tests: opening, sizing, rendering (incl. rotation and
//! tiling equivalence), outline, malformed corpus, and large-document
//! behavior (open a 1000-page doc and render a distant page cheaply).

mod support;

use std::sync::Arc;

use support::{make_pdf, palette};
use vdf_core::{Point, Rect, Rotation};
use vdf_pdf::{MupdfEngine, PdfError};

fn engine() -> MupdfEngine {
    MupdfEngine::new(256 << 20).expect("engine")
}

/// The page-unique mark in page space (y-down, zoom 1): user (484,700) 64×64
/// → page space y ∈ [792−764, 792−700] = [28, 92].
const MARK: Rect = Rect {
    min: Point { x: 484.0, y: 28.0 },
    max: Point { x: 548.0, y: 92.0 },
};

fn render_page(
    worker: &mupdf_sys::RawContext,
    doc: &vdf_pdf::PdfDocument,
    dl: &std::sync::Arc<vdf_pdf::PageDisplayList>,
    zoom: f64,
    rot: Rotation,
    region: Rect,
) -> Vec<u8> {
    doc.render_region(worker, dl, zoom, rot, region)
        .expect("render")
}

fn pixel(buf: &[u8], w: usize, x: usize, y: usize) -> [u8; 4] {
    let off = (y * w + x) * 4;
    [buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]
}

#[test]
fn open_count_and_sizes() {
    let engine = engine();
    let bytes = Arc::new(make_pdf(5));
    let doc = engine.open((*bytes).clone()).unwrap();
    assert_eq!(doc.page_count().unwrap(), 5);
    assert!(!doc.needs_password());
    let (w, h) = doc.page_size(0).unwrap();
    assert!(
        (w - 612.0).abs() < 0.5 && (h - 792.0).abs() < 0.5,
        "got {w}x{h}"
    );
    // out-of-range is a clean error, not a panic
    assert!(doc.page_size(99).is_err());
}

#[test]
fn render_marks_are_page_unique() {
    let engine = engine();
    let bytes = Arc::new(make_pdf(3));
    let worker = engine.worker_context().unwrap();
    let doc = engine.open((*bytes).clone()).unwrap();
    let dl0 = doc.load_display_list(0).unwrap();
    let dl2 = doc.load_display_list(2).unwrap();
    let page0 = render_page(&worker, &doc, &dl0, 1.0, Rotation::D0, MARK);
    let page2 = render_page(&worker, &doc, &dl2, 1.0, Rotation::D0, MARK);

    let (r0, g0, b0) = palette(0);
    let (r2, g2, b2) = palette(2);
    let mid0 = pixel(&page0, 64, 32, 32);
    let mid2 = pixel(&page2, 64, 32, 32);
    let check = |p: [u8; 4], c: (f32, f32, f32), what: &str| {
        assert!(
            (p[0] as i32 - (c.0 * 255.0) as i32).abs() < 12
                && (p[1] as i32 - (c.1 * 255.0) as i32).abs() < 12
                && (p[2] as i32 - (c.2 * 255.0) as i32).abs() < 12
                && p[3] == 255,
            "{what}: expected {:?}, got {p:?}",
            c
        );
    };
    check(mid0, (r0, g0, b0), "page 0 mark");
    check(mid2, (r2, g2, b2), "page 2 mark");
}

#[test]
fn rotation_quarter_turns_move_the_mark_correctly() {
    let engine = engine();
    let bytes = Arc::new(make_pdf(1));
    let worker = engine.worker_context().unwrap();
    let (r, g, b) = palette(0);

    // Page space (y-down): mark at top-right (484..548, 28..92).
    // D0: full page render → mark top-right.
    let doc = engine.open((*bytes).clone()).unwrap();
    let dl = doc.load_display_list(0).unwrap();
    let full = Rect::from_size(612.0, 792.0);
    let img = render_page(&worker, &doc, &dl, 1.0, Rotation::D0, full);
    let m = pixel(&img, 612, 515, 60);
    assert!(
        m[0] > 200 && m[1] < 60 && m[2] < 60,
        "D0 top-right red, got {m:?}"
    );

    // D90 (view clockwise): page top-right → view bottom-right; view 792×612.
    // R=1 mapping (x,y) → (H−y, x): mark x'∈[792−92, 792−28], y'∈[484,548].
    let img = render_page(
        &worker,
        &doc,
        &dl,
        1.0,
        Rotation::D90,
        Rect::from_size(792.0, 612.0),
    );
    let m = pixel(&img, 792, 732, 516);
    assert!(
        (m[0] as i32 - (r * 255.0) as i32).abs() < 12
            && (m[1] as i32 - (g * 255.0) as i32).abs() < 12
            && (m[2] as i32 - (b * 255.0) as i32).abs() < 12,
        "D90: mark must sit at view bottom-right, got {m:?}"
    );

    // D180: (x,y) → (W−x, H−y): mark at x'∈[64,128], y'∈[700,764].
    let img = render_page(
        &worker,
        &doc,
        &dl,
        1.0,
        Rotation::D180,
        Rect::from_size(612.0, 792.0),
    );
    let m = pixel(&img, 612, 96, 732);
    assert!(
        m[0] > 200 && m[1] < 60,
        "D180: mark at bottom-left, got {m:?}"
    );

    // D270: (x,y) → (y, W−x): mark at x'∈[28,92], y'∈[64,128].
    let img = render_page(
        &worker,
        &doc,
        &dl,
        1.0,
        Rotation::D270,
        Rect::from_size(792.0, 612.0),
    );
    let m = pixel(&img, 792, 60, 96);
    assert!(m[0] > 200 && m[1] < 60, "D270: mark at top-left, got {m:?}");
}

#[test]
fn zoom_scales_the_mark() {
    let engine = engine();
    let bytes = Arc::new(make_pdf(1));
    let worker = engine.worker_context().unwrap();
    // zoom 2 → mark occupies (968..1096, 56..184) in device px.
    let region = Rect {
        min: Point { x: 968.0, y: 56.0 },
        max: Point {
            x: 1096.0,
            y: 184.0,
        },
    };
    let doc = engine.open((*bytes).clone()).unwrap();
    let dl = doc.load_display_list(0).unwrap();
    let img = render_page(&worker, &doc, &dl, 2.0, Rotation::D0, region);
    let m = pixel(&img, 128, 64, 64);
    assert!(
        m[0] > 200 && m[1] < 60 && m[2] < 60,
        "zoom 2 mark, got {m:?}"
    );
}

#[test]
fn tiled_render_matches_full_render() {
    // Render a page at zoom 2 as one region and as a grid of 256×256 tiles;
    // stitched tiles must equal the full render within a small tolerance
    // (anti-aliasing may differ a hair at tile clip edges).
    let engine = engine();
    let bytes = Arc::new(make_pdf(1));
    let worker = engine.worker_context().unwrap();
    let zoom = 2.0;
    let full_size = (1224usize, 1584usize);

    let doc = engine.open((*bytes).clone()).unwrap();
    let dl = doc.load_display_list(0).unwrap();
    let full = doc
        .render_region(
            &worker,
            &dl,
            zoom,
            Rotation::D0,
            Rect::from_size(full_size.0 as f64, full_size.1 as f64),
        )
        .unwrap();

    let tile_px: f64 = 256.0;
    let mut stitched = vec![255u8; full_size.0 * full_size.1 * 4];
    let mut ty = 0.0;
    while ty < full_size.1 as f64 {
        let mut tx = 0.0;
        while tx < full_size.0 as f64 {
            let w = tile_px.min(full_size.0 as f64 - tx);
            let h = tile_px.min(full_size.1 as f64 - ty);
            let region = Rect {
                min: Point { x: tx, y: ty },
                max: Point {
                    x: tx + w,
                    y: ty + h,
                },
            };
            let tile = doc
                .render_region(&worker, &dl, zoom, Rotation::D0, region)
                .unwrap();
            let tw = w as usize;
            for row in 0..h as usize {
                let src = row * tw * 4;
                let dst = ((ty as usize + row) * full_size.0 + tx as usize) * 4;
                stitched[dst..dst + tw * 4].copy_from_slice(&tile[src..src + tw * 4]);
            }
            tx += tile_px;
        }
        ty += tile_px;
    }

    // Compare with tolerance: interior pixels must match closely; allow a
    // little extra slack at tile boundaries.
    let mut max_diff = 0u32;
    let mut boundary_max = 0u32;
    for y in 0..full_size.1 {
        for x in 0..full_size.0 {
            let off = (y * full_size.0 + x) * 4;
            for c in 0..4 {
                let d = (full[off + c] as i32 - stitched[off + c] as i32).unsigned_abs();
                let on_boundary = x % 256 == 0 || y % 256 == 0 || x % 256 == 255 || y % 256 == 255;
                if on_boundary {
                    boundary_max = boundary_max.max(d);
                } else {
                    max_diff = max_diff.max(d);
                }
            }
        }
    }
    assert!(
        max_diff <= 3,
        "interior pixel diff {max_diff} exceeds tolerance"
    );
    assert!(
        boundary_max <= 16,
        "tile-boundary diff {boundary_max} exceeds tolerance"
    );
}

#[test]
fn outline_parses() {
    // The fixture generator emits no outline; mupdf must report none
    // (empty vec), not an error.
    let engine = engine();
    let bytes = make_pdf(2);
    let doc = engine.open(bytes).unwrap();
    let outline = doc.outline().unwrap();
    assert!(
        outline.is_empty(),
        "fixture has no outline, got {outline:?}"
    );
}

#[test]
fn malformed_corpus_errors_typed_not_panics() {
    let engine = engine();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("garbage", b"not a pdf at all, just text".to_vec()),
        ("truncated-xref", {
            let mut b = make_pdf(2);
            b.truncate(b.len() / 2);
            b
        }),
        ("bad-magic", {
            let mut b = make_pdf(1);
            b.splice(0..8, b"%PDF-9.9\n".iter().copied());
            b
        }),
        ("random-bytes", vec![0x41u8; 4096]),
    ];
    for (name, bytes) in cases {
        let result = engine.open(bytes);
        match result {
            Err(PdfError::Malformed(_)) => {}
            Err(PdfError::Engine(_)) => {}
            // mupdf may repair mildly broken files; the repaired doc must
            // still answer basic queries without panicking.
            Ok(doc) => {
                let _ = doc.page_count();
            }
            Err(other) => panic!("{name}: unexpected error {other}"),
        }
    }
}

#[test]
fn large_document_opens_fast_and_renders_distant_page() {
    let engine = engine();
    let t_open = std::time::Instant::now();
    let bytes = Arc::new(make_pdf(1000));
    let build = t_open.elapsed();
    let doc = engine.open((*bytes).clone()).expect("open 1000-page");
    let t_count = std::time::Instant::now();
    let pages = doc.page_count().unwrap();
    let count_elapsed = t_count.elapsed();
    assert_eq!(pages, 1000);
    // Page count must be cheap (page tree, not a full parse sweep).
    assert!(
        count_elapsed.as_millis() < 200,
        "count_pages took {count_elapsed:?} — must not sweep all pages"
    );

    // Render a distant page without touching pages 0..998.
    let t_render = std::time::Instant::now();
    let worker = engine.worker_context().unwrap();
    let dl = doc.load_display_list(999).unwrap();
    let img = render_page(&worker, &doc, &dl, 1.0, Rotation::D0, MARK);
    let render_elapsed = t_render.elapsed();
    assert_eq!(img.len(), 64 * 64 * 4);
    let (r, _, _) = palette(999);
    let m = pixel(&img, 64, 32, 32);
    assert!(m[0] as f32 > r * 200.0, "page 999 mark must render");
    println!(
        "large-doc: build {build:?}, count {count_elapsed:?}, distant render {render_elapsed:?}"
    );
}

#[test]
fn worker_contexts_render_concurrently() {
    use std::sync::Barrier;
    let engine = Arc::new(engine());
    let bytes = Arc::new(make_pdf(8));
    // App threading model (MASTER_PLAN.md §8, MuPDF rule 2): opens and
    // display-list builds are serial on the document actor; rendering runs
    // display lists concurrently on cloned worker contexts.
    let doc = Arc::new(engine.open((*bytes).clone()).unwrap());
    let dls: Vec<_> = (0..8u32)
        .map(|p| doc.load_display_list(p).expect("actor load dl"))
        .collect();
    let barrier = Arc::new(Barrier::new(4));
    let mut handles = Vec::new();
    for t in 0..4 {
        let engine = Arc::clone(&engine);
        let doc = Arc::clone(&doc);
        let dl = dls[(t * 2) as usize].clone();
        let barrier = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            let worker = engine.worker_context().unwrap();
            barrier.wait();
            // All four threads run different display lists at the same time.
            let page = (t * 2) as u32;
            let img = doc
                .render_region(&worker, &dl, 1.0, Rotation::D0, MARK)
                .expect("concurrent render");
            assert_eq!(img.len(), 64 * 64 * 4);
            let (r, g, b) = palette(page as usize);
            let m = pixel(&img, 64, 32, 32);
            assert!(
                (m[0] as i32 - (r * 255.0) as i32).abs() < 12
                    && (m[1] as i32 - (g * 255.0) as i32).abs() < 12
                    && (m[2] as i32 - (b * 255.0) as i32).abs() < 12,
                "thread {t} page {page} mark mismatch: {m:?} vs ({r},{g},{b})"
            );
        }));
    }
    for h in handles {
        h.join().expect("worker thread");
    }
}
