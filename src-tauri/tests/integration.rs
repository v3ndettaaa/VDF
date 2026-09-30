//! Cross-crate integration tests: prove the workspace wires together and the
//! core invariants hold through the public APIs another agent will build on.
//!
//! (Rendering golden tests, tile scheduling, and PDF behavior start in M1;
//! these are the M0 invariants.)

use std::collections::HashSet;

use vdf_core::{DocumentId, ObjectId, PageIndex, Point, Rect, Rotation, TileKey, ZoomFactor};
use vdf_document::{AddObject, Document, History, SetObjectOpacity};
use vdf_persist::atomic_write;
use vdf_render::{FinishedTile, MemoryTransport, RenderTransport};
use vdf_search::SearchQuery;

fn doc_with_object() -> (Document, History, ObjectId) {
    let mut doc = Document::new("integration");
    let page = doc.add_page((612.0, 792.0));
    let obj = doc.make_envelope(page, Rect::from_center_size(100.0, 100.0, 10.0, 10.0));
    let id = obj.id;
    let mut hist = History::new(100);
    hist.apply(Box::new(AddObject::new(obj)), &mut doc).unwrap();
    (doc, hist, id)
}

#[test]
fn coordinate_chain_round_trips() {
    // PDF space → document space → viewport space → back
    let pdf = Point::new(100.0, 200.0);
    let page_offset = vdf_core::Affine2::from_translation(0.0, 792.0); // y-flip at page top
    let view = vdf_core::Affine2::from_scale(1.5, -1.5); // zoom 150%, y-down flip
    let fwd = page_offset.compose(view);
    let back = fwd.try_invert().unwrap();
    let round = back.apply(fwd.apply(pdf));
    assert!((round.x - pdf.x).abs() < 1e-9);
    assert!((round.y - pdf.y).abs() < 1e-9);
}

#[test]
fn undo_redo_through_public_api() {
    let (mut doc, mut hist, id) = doc_with_object();
    hist.apply(Box::new(SetObjectOpacity::new(id, 0.3)), &mut doc)
        .unwrap();
    assert_eq!(doc.object(id).unwrap().opacity, 0.3);
    hist.undo(&mut doc).unwrap();
    assert_eq!(doc.object(id).unwrap().opacity, 1.0);
    hist.redo(&mut doc).unwrap();
    assert_eq!(doc.object(id).unwrap().opacity, 0.3);
}

#[test]
fn stale_tile_never_wins() {
    let t = MemoryTransport::new();
    let doc = DocumentId(77);
    let key = TileKey::new(PageIndex(0), 8, Rotation::D0, 0, 0);
    let mk = |generation: u64| FinishedTile {
        doc,
        key,
        generation: vdf_core::RenderGeneration(generation),
        width: 8,
        height: 8,
        rgba: std::sync::Arc::new(vec![0u8; 8 * 8 * 4]),
    };
    t.publish_tile(mk(2)).unwrap(); // newer lands first
    t.publish_tile(mk(1)).unwrap(); // older straggler arrives late
    assert_eq!(
        t.generation_of(doc, &key),
        Some(vdf_core::RenderGeneration(2))
    );
    assert_eq!(t.stats().stale_dropped, 1);
}

#[test]
fn atomic_save_leaves_original_valid_on_simulated_retry() {
    let dir = std::env::temp_dir().join(format!("vdf-it-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("doc.bin");
    atomic_write(&target, b"v1", false).unwrap();
    // "interrupted" second save writes different content — still atomic
    atomic_write(&target, b"v2-longer-content", true).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"v2-longer-content");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn search_modes_via_public_api() {
    let q = SearchQuery::new("loop").whole_word(true);
    assert!(q.matches("event loop here").unwrap());
    assert!(
        !q.matches("the loops package").unwrap(),
        "'loop' inside 'loops' is not a whole word"
    );
    let re = SearchQuery::new("l[o0]op").regex(true);
    assert!(re.matches("l0op").unwrap());
    assert!(
        re.matches("lOop").unwrap(),
        "case folding applies to regex too"
    );
}

#[test]
fn many_documents_coexist_with_unique_ids() {
    // Object ids are per-document scoped (each document owns a generator);
    // cross-document uniqueness is carried by DocumentId.
    let mut doc_ids: HashSet<vdf_core::DocumentId> = HashSet::new();
    for i in 0..25 {
        let (doc, _hist, id) = doc_with_object();
        assert!(
            doc_ids.insert(doc.id),
            "document ids must be unique across instances (iteration {i})"
        );
        assert_eq!(doc.pages().len(), 1);
        assert_eq!(doc.object_count(), 1);
        assert!(doc.object(id).is_some());
    }
}

#[test]
fn zoom_clamps_across_ladder() {
    assert_eq!(ZoomFactor::new(500.0).get(), 64.0);
    assert_eq!(ZoomFactor::new(0.001).get(), 0.05);
    assert!((ZoomFactor::new(1.0).percent() - 100.0).abs() < f64::EPSILON);
}
