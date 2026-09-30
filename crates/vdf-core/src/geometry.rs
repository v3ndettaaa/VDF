//! f64 geometry primitives: points, axis-aligned rects, 2D affine transforms.
//!
//! The affine matrix stores the standard PostScript/PDF layout
//! `[a c e; b d f; 0 0 1]`, so `apply` is:
//!
//! ```text
//! x' = a*x + c*y + e
//! y' = b*x + d*y + f
//! ```
//!
//! Composition convention: `a.compose(&b)` means "apply `a` first, then `b`".
//! This mirrors the PDF-space → document-space → viewport-space → screen-space
//! chain (MASTER_PLAN.md §7), where each stage composes onto the previous one.

use crate::error::{VdfError, VdfResult};

/// Tolerance below which an affine is treated as non-invertible.
const MIN_ABS_DETERMINANT: f64 = 1e-12;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Euclidean distance to another point.
    pub fn distance(self, other: Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }

    /// Linear interpolation (t = 0 → self, t = 1 → other).
    pub fn lerp(self, other: Point, t: f64) -> Point {
        Point::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }
}

/// Axis-aligned rectangle stored as min/max corners. An empty rect has
/// `min > max` on some axis; width/height are reported as 0.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Rect {
    pub const fn new(min: Point, max: Point) -> Self {
        Self { min, max }
    }

    pub fn from_size(w: f64, h: f64) -> Self {
        Self {
            min: Point::ZERO,
            max: Point::new(w, h),
        }
    }

    pub fn from_center_size(cx: f64, cy: f64, w: f64, h: f64) -> Self {
        Self {
            min: Point::new(cx - w / 2.0, cy - h / 2.0),
            max: Point::new(cx + w / 2.0, cy + h / 2.0),
        }
    }

    pub fn width(self) -> f64 {
        (self.max.x - self.min.x).max(0.0)
    }

    pub fn height(self) -> f64 {
        (self.max.y - self.min.y).max(0.0)
    }

    pub fn is_empty(self) -> bool {
        self.max.x < self.min.x || self.max.y < self.min.y
    }

    pub fn area(self) -> f64 {
        self.width() * self.height()
    }

    pub fn center(self) -> Point {
        Point::new(
            (self.min.x + self.max.x) / 2.0,
            (self.min.y + self.max.y) / 2.0,
        )
    }

    pub fn contains(self, p: Point) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    pub fn contains_rect(self, other: Rect) -> bool {
        other.min.x >= self.min.x
            && other.max.x <= self.max.x
            && other.min.y >= self.min.y
            && other.max.y <= self.max.y
    }

    pub fn intersects(self, other: Rect) -> bool {
        !self.intersect(other).is_empty()
    }

    pub fn union(self, other: Rect) -> Rect {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        Rect {
            min: Point::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Point::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    pub fn intersect(self, other: Rect) -> Rect {
        Rect {
            min: Point::new(self.min.x.max(other.min.x), self.min.y.max(other.min.y)),
            max: Point::new(self.max.x.min(other.max.x), self.max.y.min(other.max.y)),
        }
    }

    /// Grows the rect by a fixed margin on all sides.
    pub fn inflated(self, m: f64) -> Rect {
        Rect {
            min: Point::new(self.min.x - m, self.min.y - m),
            max: Point::new(self.max.x + m, self.max.y + m),
        }
    }

    /// Smallest rect containing all points (empty rect if none).
    pub fn bounding_box(points: impl IntoIterator<Item = Point>) -> Rect {
        let mut iter = points.into_iter();
        let Some(first) = iter.next() else {
            return Rect::new(
                Point::new(f64::INFINITY, f64::INFINITY),
                Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
            );
        };
        let mut r = Rect::new(first, first);
        for p in iter {
            r = r.union(Rect::new(p, p));
        }
        r
    }

    /// Axis-aligned bounding box after an affine transform. Exact for
    /// axis-aligned transforms; conservative (outer bound) for rotations,
    /// which is what culling/hit-testing needs.
    pub fn transformed(self, m: Affine2) -> Rect {
        let corners = [
            m.apply(self.min),
            m.apply(Point::new(self.max.x, self.min.y)),
            m.apply(Point::new(self.min.x, self.max.y)),
            m.apply(self.max),
        ];
        Rect::bounding_box(corners)
    }
}

/// 2D affine transform `[a c e; b d f; 0 0 1]` in f64.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine2 {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine2 {
    pub const IDENTITY: Affine2 = Affine2 {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub const fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self { a, b, c, d, e, f }
    }

    pub fn from_translation(tx: f64, ty: f64) -> Self {
        Self {
            e: tx,
            f: ty,
            ..Self::IDENTITY
        }
    }

    pub fn from_scale(sx: f64, sy: f64) -> Self {
        Self {
            a: sx,
            d: sy,
            ..Self::IDENTITY
        }
    }

    pub fn from_rotation(radians: f64) -> Self {
        let (s, c) = radians.sin_cos();
        Self {
            a: c,
            b: s,
            c: -s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Applies this transform to a point.
    pub fn apply(self, p: Point) -> Point {
        Point::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }

    /// Applies the linear part only (no translation) — for direction vectors.
    pub fn apply_vector(self, x: f64, y: f64) -> (f64, f64) {
        (self.a * x + self.c * y, self.b * x + self.d * y)
    }

    /// `self.compose(&other)` = apply `self` first, then `other`.
    pub fn compose(self, other: Affine2) -> Affine2 {
        Affine2 {
            a: other.a * self.a + other.c * self.b,
            b: other.b * self.a + other.d * self.b,
            c: other.a * self.c + other.c * self.d,
            d: other.b * self.c + other.d * self.d,
            e: other.a * self.e + other.c * self.f + other.e,
            f: other.b * self.e + other.d * self.f + other.f,
        }
    }

    pub fn determinant(self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    pub fn is_finite(self) -> bool {
        [self.a, self.b, self.c, self.d, self.e, self.f]
            .iter()
            .all(|v| v.is_finite())
    }

    /// Analytic inverse; `None` for singular or non-finite transforms.
    pub fn invert(self) -> Option<Affine2> {
        if !self.is_finite() {
            return None;
        }
        let det = self.determinant();
        if det.abs() < MIN_ABS_DETERMINANT {
            return None;
        }
        Some(Affine2 {
            a: self.d / det,
            b: -self.b / det,
            c: -self.c / det,
            d: self.a / det,
            e: (self.c * self.f - self.d * self.e) / det,
            f: (self.b * self.e - self.a * self.f) / det,
        })
    }

    /// Uniform scale factor for line widths under this transform.
    /// Exact for uniform scale/rotation; uses the mean for anisotropic scales.
    pub fn mean_scale(self) -> f64 {
        let sx = (self.a * self.a + self.b * self.b).sqrt();
        let sy = (self.c * self.c + self.d * self.d).sqrt();
        (sx + sy) / 2.0
    }

    pub fn try_invert(self) -> VdfResult<Affine2> {
        self.invert().ok_or_else(|| {
            VdfError::Geometry(format!(
                "affine not invertible (det={:?}, non-finite={})",
                self.determinant(),
                !self.is_finite()
            ))
        })
    }
}

impl Default for Affine2 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    fn assert_close(a: f64, b: f64, what: &str) {
        assert!(
            (a - b).abs() < EPS,
            "{what}: {a} vs {b} (diff exceeds {EPS})"
        );
    }

    fn assert_point_close(p: Point, q: Point, what: &str) {
        assert_close(p.x, q.x, &format!("{what}.x"));
        assert_close(p.y, q.y, &format!("{what}.y"));
    }

    #[test]
    fn identity_is_neutral() {
        let p = Point::new(3.5, -7.25);
        assert_point_close(Affine2::IDENTITY.apply(p), p, "identity");
    }

    #[test]
    fn translation_moves() {
        let t = Affine2::from_translation(10.0, -4.0);
        assert_point_close(
            t.apply(Point::ZERO),
            Point::new(10.0, -4.0),
            "translate origin",
        );
        assert_point_close(
            t.apply(Point::new(1.0, 1.0)),
            Point::new(11.0, -3.0),
            "translate point",
        );
    }

    #[test]
    fn rotation_quarter_turn() {
        let r = Affine2::from_rotation(std::f64::consts::FRAC_PI_2);
        // (1, 0) rotated +90° (y-up math convention) → (0, 1)
        assert_point_close(
            r.apply(Point::new(1.0, 0.0)),
            Point::new(0.0, 1.0),
            "rot90 of (1,0)",
        );
        // and (0,1) → (-1,0)
        assert_point_close(
            r.apply(Point::new(0.0, 1.0)),
            Point::new(-1.0, 0.0),
            "rot90 of (0,1)",
        );
    }

    #[test]
    fn scale_and_translate_compose_in_order() {
        let s = Affine2::from_scale(2.0, 3.0);
        let t = Affine2::from_translation(5.0, 7.0);
        // s.compose(t): scale first, then translate → (2,3) → (7,10)
        assert_point_close(
            s.compose(t).apply(Point::new(1.0, 1.0)),
            Point::new(7.0, 10.0),
            "s∘t",
        );
        // t.compose(s): translate first, then scale → (6,8) → (12,24)
        assert_point_close(
            t.compose(s).apply(Point::new(1.0, 1.0)),
            Point::new(12.0, 24.0),
            "t∘s",
        );
    }

    #[test]
    fn invert_round_trip_manual() {
        let m = Affine2::new(2.0, 0.5, -0.25, 1.75, 12.0, -3.0);
        let inv = m.try_invert().expect("invertible");
        let p = Point::new(4.25, -9.5);
        assert_point_close(inv.apply(m.apply(p)), p, "inv∘m");
        assert_point_close(m.apply(inv.apply(p)), p, "m∘inv");
    }

    #[test]
    fn singular_and_non_finite_do_not_invert() {
        assert!(Affine2::from_scale(0.0, 1.0).invert().is_none());
        assert!(
            Affine2::new(f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0)
                .invert()
                .is_none()
        );
        assert!(
            Affine2::new(f64::INFINITY, 0.0, 0.0, 1.0, 0.0, 0.0)
                .invert()
                .is_none()
        );
    }

    #[test]
    fn rect_ops() {
        let a = Rect::from_size(10.0, 10.0);
        let b = Rect::new(Point::new(5.0, 5.0), Point::new(15.0, 15.0));
        let i = a.intersect(b);
        assert_close(i.min.x, 5.0, "intersect.min.x");
        assert_close(i.width(), 5.0, "intersect.width");
        let u = a.union(b);
        assert_close(u.width(), 15.0, "union.width");
        assert!(a.contains(Point::new(0.0, 0.0)));
        assert!(!a.contains(Point::new(10.5, 0.0)));
        assert!(a.intersects(b));
        assert!(!a.intersects(b.translated(Point::new(100.0, 100.0))));
        assert_eq!(
            Rect::bounding_box([Point::new(1.0, 2.0), Point::new(-3.0, 5.0)])
                .min
                .x,
            -3.0
        );
    }

    impl Rect {
        fn translated(self, t: Point) -> Rect {
            Rect {
                min: Point::new(self.min.x + t.x, self.min.y + t.y),
                max: Point::new(self.max.x + t.x, self.max.y + t.y),
            }
        }
    }

    #[test]
    fn transformed_rect_contains_transformed_corners() {
        let m = Affine2::from_rotation(0.7).compose(Affine2::from_translation(3.0, 4.0));
        let r = Rect::from_center_size(2.0, -1.0, 6.0, 4.0);
        let bb = r.transformed(m);
        for corner in [
            r.min,
            Point::new(r.max.x, r.min.y),
            Point::new(r.min.x, r.max.y),
            r.max,
        ] {
            assert!(
                bb.contains(m.apply(corner)),
                "bounding box must contain transformed corner"
            );
        }
    }

    #[test]
    fn point_helpers() {
        assert_close(
            Point::new(0.0, 0.0).distance(Point::new(3.0, 4.0)),
            5.0,
            "distance",
        );
        assert_point_close(
            Point::new(0.0, 0.0).lerp(Point::new(10.0, 20.0), 0.25),
            Point::new(2.5, 5.0),
            "lerp",
        );
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    fn finite_affine() -> impl Strategy<Value = Affine2> {
        (
            (-8.0f64..8.0, -8.0f64..8.0),
            (-8.0f64..8.0, -8.0f64..8.0),
            (-100.0f64..100.0, -100.0f64..100.0),
        )
            .prop_map(|((a, b), (c, d), (e, f))| Affine2::new(a, b, c, d, e, f))
            // keep a healthy margin away from singular
            .prop_filter("invertible", |m| m.determinant().abs() > 1e-3)
    }

    proptest! {
        #[test]
        fn invert_round_trip(m in finite_affine(), x in -1e3f64..1e3, y in -1e3f64..1e3) {
            let p = Point::new(x, y);
            let inv = m.invert().expect("filter guarantees invertibility");
            prop_assert!((inv.apply(m.apply(p)).x - p.x).abs() < 1e-6);
            prop_assert!((inv.apply(m.apply(p)).y - p.y).abs() < 1e-6);
            prop_assert!((m.apply(inv.apply(p)).x - p.x).abs() < 1e-6);
            prop_assert!((m.apply(inv.apply(p)).y - p.y).abs() < 1e-6);
        }

        #[test]
        fn compose_with_inverse_is_identity(m in finite_affine(), x in -1e3f64..1e3, y in -1e3f64..1e3) {
            let p = Point::new(x, y);
            let inv = m.invert().unwrap();
            prop_assert!((m.compose(inv).apply(p).x - p.x).abs() < 1e-6);
            prop_assert!((m.compose(inv).apply(p).y - p.y).abs() < 1e-6);
        }

        #[test]
        fn union_commutative_and_contains_operands(
            ax in -50f64..50.0, ay in -50f64..50.0, aw in 0f64..50.0, ah in 0f64..50.0,
            bx in -50f64..50.0, by in -50f64..50.0, bw in 0f64..50.0, bh in 0f64..50.0,
        ) {
            let a = Rect::from_center_size(ax, ay, aw, ah);
            let b = Rect::from_center_size(bx, by, bw, bh);
            let u1 = a.union(b);
            let u2 = b.union(a);
            prop_assert!((u1.min.x - u2.min.x).abs() < 1e-9);
            prop_assert!((u1.max.y - u2.max.y).abs() < 1e-9);
            if !a.is_empty() { prop_assert!(u1.contains_rect(a)); }
            if !b.is_empty() { prop_assert!(u1.contains_rect(b)); }
        }

        #[test]
        fn intersection_subset_of_operands(
            ax in -50f64..50.0, ay in -50f64..50.0, aw in 0f64..50.0, ah in 0f64..50.0,
            bx in -50f64..50.0, by in -50f64..50.0, bw in 0f64..50.0, bh in 0f64..50.0,
        ) {
            let a = Rect::from_center_size(ax, ay, aw, ah);
            let b = Rect::from_center_size(bx, by, bw, bh);
            let i = a.intersect(b);
            if !i.is_empty() {
                prop_assert!(a.contains_rect(i));
                prop_assert!(b.contains_rect(i));
            }
        }
    }
}
