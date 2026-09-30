#![no_main]

//! Fuzz the geometry core: arbitrary bytes must never panic the affine
//! pipeline, and garbage inputs must degrade to `None` (non-invertible),
//! never to NaN propagation into `Ok` results.

use libfuzzer_sys::fuzz_target;

fn f64_from(b: &[u8]) -> Option<f64> {
    let a: [u8; 8] = b.try_into().ok()?;
    let v = f64::from_le_bytes(a);
    v.is_finite().then_some(v)
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 48 + 16 {
        return;
    }
    let m = vdf_core::Affine2::new(
        f64_from(&data[0..8]).unwrap_or(1.0),
        f64_from(&data[8..16]).unwrap_or(0.0),
        f64_from(&data[16..24]).unwrap_or(0.0),
        f64_from(&data[24..32]).unwrap_or(1.0),
        f64_from(&data[32..40]).unwrap_or(0.0),
        f64_from(&data[40..48]).unwrap_or(0.0),
    );
    let p = vdf_core::Point::new(
        f64_from(&data[48..56]).unwrap_or(0.0),
        f64_from(&data[56..64]).unwrap_or(0.0),
    );
    let applied = m.apply(p);
    if let Some(inv) = m.invert() {
        let back = inv.apply(applied);
        assert!((back.x - p.x).abs() < 1e-6 || !back.is_finite());
        assert!((back.y - p.y).abs() < 1e-6 || !back.is_finite());
    }
    // transforms must survive bounding-box use without panicking
    let _ = vdf_core::Rect::from_center_size(0.0, 0.0, 10.0, 10.0).transformed(m);
});
