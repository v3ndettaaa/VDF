//! vdf-bench — headless scenario harness (MASTER_PLAN.md §13).
//!
//! Scenarios drive the real core crates without a GUI. M0 ships the harness
//! shape (scenario list, JSON output, PASS/WARN/FAIL status) and a smoke
//! scenario over the document model. The viewer benchmarks (startup, open,
//! scroll-jump, zoom ladder, ink-while-rendering) arrive with M1 when there
//! is rendering to measure.
//!
//! Gates: M0/M1 runs *establish baselines* (`bench/baselines.json`); gates
//! are only enforced once real-hardware baselines exist.

use std::time::Instant;

use vdf_core::Rect;
use vdf_document::{AddObject, Document, History, SetObjectOpacity};

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

const SCENARIOS: &[&str] = &["smoke"];

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
        other => {
            eprintln!("unknown scenario {other:?}; available: {SCENARIOS:?}");
            std::process::exit(2);
        }
    }
}

/// Document-model churn: build a document, add objects, apply and undo
/// commands. Exercises ids, objects, commands, and history end to end.
fn run_smoke() {
    let t0 = Instant::now();

    let mut doc = Document::new("bench");
    let page = doc.add_page((612.0, 792.0));
    let mut hist = History::new(10_000);

    let mut ids = Vec::new();
    for i in 0..200 {
        let obj = doc.make_envelope(
            page,
            Rect::from_center_size((i % 20) as f64 * 10.0, (i / 20) as f64 * 10.0, 8.0, 8.0),
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

    // verify round-trip behavior: undo all, redo all, then confirm state
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

    println!(
        "{}",
        serde_json::json!({
            "scenario": "smoke",
            "duration_ms": elapsed.as_secs_f64() * 1000.0,
            "objects": restored_objects,
            "undo_depth": undo_depth,
            "status": status.as_str(),
            "gated": false,
            "note": "baseline-establishing run; gates activate once baselines exist",
        })
    );
    if status == Status::Fail {
        std::process::exit(1);
    }
}
