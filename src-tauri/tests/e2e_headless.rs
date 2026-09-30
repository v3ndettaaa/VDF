//! Headless end-to-end test of the app layer: open → viewport → poll →
//! tile bytes → thumbnails → outline → navigate. This drives the same
//! `AppCore`/`DocCore` methods the Tauri commands delegate to — no GUI
//! (the GUI smoke is tauri-driver, M6).

use std::sync::Arc;
use std::time::{Duration, Instant};

use vdf_app_lib::commands::parse_tile_key;
use vdf_app_lib::state::AppCore;
use vdf_render::PageMode;

#[path = "../../crates/vdf-pdf/tests/support/mod.rs"]
mod fixture;

#[test]
fn open_viewport_poll_tile_pipeline() {
    let core = AppCore::new().unwrap();
    let bytes = fixture::make_pdf(24);
    let dir = std::env::temp_dir().join(format!("vdf-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("e2e.pdf");
    std::fs::write(&path, &bytes).unwrap();

    // open
    let t_open = Instant::now();
    let doc = core.open_document(path.to_str().unwrap(), bytes).unwrap();
    let open_ms = t_open.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(doc.page_count, 24);

    // viewport update (as the UI would send)
    doc.viewport(1200.0, 900.0, 1.0, 0.0, 0.0, 0.0, false);

    // poll until the first tiles are ready
    let t_first = Instant::now();
    let mut ready = 0usize;
    let mut first_keys: Vec<String> = Vec::new();
    for _ in 0..3000 {
        let (ready_tiles, _stale, _rendered) = doc.poll();
        ready += ready_tiles.len();
        first_keys.extend(ready_tiles);
        if ready >= 6 {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let first_ms = t_first.elapsed().as_secs_f64() * 1000.0;
    assert!(ready >= 6, "expected ready tiles, got {ready}");

    // tile bytes via the same lookup the protocol uses
    let key_str = first_keys
        .iter()
        .find(|k| parse_tile_key(k).is_some())
        .expect("a parseable ready key")
        .clone();
    let key = parse_tile_key(&key_str).unwrap();
    let (w, h, px) = doc.scheduler.tile_pixels(&key).expect("tile pixels");
    assert_eq!(px.len(), (w * h * 4) as usize);
    assert!(
        px.iter().any(|&b| b != 0),
        "tile must contain rendered pixels"
    );

    // layout is sane
    let layout = doc.current_layout();
    assert_eq!(layout.pages.len(), 24);
    assert!(layout.doc_size.1 > 24.0 * 792.0, "pages stacked");

    // thumbnails
    doc.request_thumbnails(&[0, 1, 2], 96);
    let mut thumbs = 0;
    for _ in 0..3000 {
        thumbs += doc.take_thumb_ready().len();
        if thumbs >= 3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(thumbs >= 3, "thumbnails rendered");
    let framed = doc.thumb(0, 96).expect("thumb 0");
    let tw = u32::from_le_bytes([framed[0], framed[1], framed[2], framed[3]]);
    assert_eq!(tw, 96);
    assert_eq!(
        framed.len(),
        8 + (tw * framed[4] as u32 * 4) as usize + 4 - 4
    ); // header + RGBA

    // outline request does not error (fixture has none)
    assert!(doc.outline().is_empty());

    // navigation + zoom + modes (same path as the commands)
    let target = doc.goto_page(10).unwrap();
    assert!(target.1 > 0.0);
    doc.pan(0.0, 50.0);
    let new_zoom = doc.zoom(2.0, (600.0, 400.0), None);
    assert!(new_zoom > 1.9, "zoom applied: {new_zoom}");
    for _ in 0..500 {
        let (ready_tiles, _, _) = doc.poll();
        if !ready_tiles.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        doc.active_page.load(std::sync::atomic::Ordering::Relaxed),
        10
    );

    doc.set_page_mode(PageMode::TwoPage);
    let layout = doc.current_layout();
    let r0 = layout.rect_of(vdf_core::PageIndex(0)).unwrap();
    let r1 = layout.rect_of(vdf_core::PageIndex(1)).unwrap();
    assert!(r1.min.x > r0.max.x, "two-page places pages side by side");

    println!(
        "e2e: open {open_ms:.1}ms, first tiles {first_ms:.1}ms, tile {w}x{h}, thumbs {thumbs}"
    );

    core.close_document(doc.handle.0);
    std::fs::remove_dir_all(&dir).ok();
    let _ = Arc::clone(&doc); // keep borrow checker quiet about ordering
}
