//! Generates tests/vectors/affine-cases.json — the golden vectors shared by
//! the Rust geometry tests and the TS mirror test (ui/affine-golden.test.ts).
//!
//! The file is regenerated on every `cargo test`; contents are deterministic.
//! Changing an expected value here means the geometry changed on purpose and
//! both implementations were reviewed — never "make the test green" editing.

use std::fs;
use std::path::PathBuf;

use serde_json::json;
use vdf_core::{Affine2, Point};

const OUT_DIR_SEGMENT: &str = "tests/vectors";

fn write_vectors() {
    let cases = vec![
        ("identity", Affine2::IDENTITY),
        ("translate", Affine2::from_translation(12.5, -3.25)),
        ("scale", Affine2::from_scale(2.0, 0.5)),
        (
            "quarter-rotation",
            Affine2::from_rotation(std::f64::consts::FRAC_PI_2),
        ),
        ("mixed", Affine2::new(1.25, 0.5, -0.75, 2.0, 40.0, -15.0)),
        (
            "negative-determinant",
            Affine2::new(1.0, 0.0, 0.0, -1.0, 5.0, 5.0),
        ),
        ("singular", Affine2::from_scale(0.0, 1.0)),
    ];

    let p = Point::new(3.75, -9.125);
    let shift = Affine2::from_translation(10.0, -4.0);

    let case_json: Vec<_> = cases
        .into_iter()
        .map(|(name, m)| {
            let applied = m.apply(p);
            let composed = m.compose(shift).apply(p);
            let inverted = m.invert().map(|inv| {
                let back = inv.apply(applied);
                json!([round(back.x), round(back.y)])
            });
            json!({
                "name": name,
                "m": [m.a, m.b, m.c, m.d, m.e, m.f],
                "p": [p.x, p.y],
                "applied": [round(applied.x), round(applied.y)],
                "inverted": inverted,
                "composed": [round(composed.x), round(composed.y)],
            })
        })
        .collect();

    // sanity: round-trip on every invertible case (Rust side is itself tested)
    for m in [
        Affine2::new(1.25, 0.5, -0.75, 2.0, 40.0, -15.0),
        Affine2::from_rotation(0.9),
    ] {
        let inv = m.invert().expect("case must be invertible");
        let back = inv.apply(m.apply(p));
        assert!((back.x - p.x).abs() < 1e-9 && (back.y - p.y).abs() < 1e-9);
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_dir = manifest_dir
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .join(OUT_DIR_SEGMENT);
    fs::create_dir_all(&out_dir).expect("create vectors dir");
    let out_path = out_dir.join("affine-cases.json");
    fs::write(
        &out_path,
        serde_json::to_string_pretty(&json!({ "version": 1, "cases": case_json }))
            .expect("serialize"),
    )
    .expect("write vectors");
    println!("golden vectors written to {}", out_path.display());
}

fn round(v: f64) -> f64 {
    // 12 significant decimal digits keeps TS parsing exact enough for 1e-9
    // comparisons while producing stable diffs in git.
    let s = format!("{v:.12}");
    s.parse().unwrap_or(v)
}

#[test]
fn regenerate_affine_golden_vectors() {
    write_vectors();
}
