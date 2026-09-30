//! Isolation test: sequential master-context use across threads (the loader
//! pattern) — page_size on a spawned thread after open on main.

use std::sync::Arc;
use vdf_pdf::MupdfEngine;

#[path = "../../vdf-pdf/tests/support/mod.rs"]
mod fixture;

#[test]
fn page_size_on_second_thread() {
    let engine = Arc::new(MupdfEngine::new(64 << 20).unwrap());
    let bytes = fixture::make_pdf(8);
    let doc = Arc::new(engine.open(bytes).unwrap());
    assert_eq!(doc.page_count().unwrap(), 8);
    let (w, _h) = doc.page_size(0).unwrap(); // main thread: fine
    assert!(w > 0.0);

    let handle = std::thread::spawn(move || {
        eprintln!("[t] spawned; calling page_size(1)");
        let r = doc.page_size(1);
        eprintln!("[t] page_size returned");
        r
    });
    let r = handle.join().expect("thread must not hang/panic");
    let (w, h) = r.expect("page_size ok");
    assert!(w > 0.0 && h > 0.0);
}
