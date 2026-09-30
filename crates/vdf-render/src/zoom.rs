//! The central zoom controller (MASTER_PLAN.md §10): every zoom input goes
//! through it; zoom is focal-point-centered and quantized for tile requests.

/// Zoom limits (fraction; 1.0 = 100%).
pub const ZOOM_MIN: f64 = 0.05;
pub const ZOOM_MAX: f64 = 64.0;

/// One central zoom state. Rust-authoritative; the compositor mirrors it
/// for immediate feedback during gestures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentZoomController {
    zoom: f64,
    /// Scroll offset (view px, y-down) of the document top-left.
    scroll: (f64, f64),
}

impl DocumentZoomController {
    pub fn new() -> Self {
        Self {
            zoom: 1.0,
            scroll: (0.0, 0.0),
        }
    }

    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    pub fn scroll(&self) -> (f64, f64) {
        self.scroll
    }

    pub fn set_scroll(&mut self, x: f64, y: f64) {
        self.scroll = (x, y);
    }

    /// Sets the zoom centered on a focal point in view coordinates.
    /// The document point under the focal point stays put on screen.
    pub fn set_zoom_focal(&mut self, new_zoom: f64, focal: (f64, f64), viewport: (f64, f64)) {
        let new_zoom = new_zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        if !new_zoom.is_finite() {
            return;
        }
        // focal point relative to the viewport = document point at
        // (focal + scroll) / old_zoom
        let fp = (focal.0 + self.scroll.0, focal.1 + self.scroll.1);
        let doc = (fp.0 / self.zoom, fp.1 / self.zoom);
        self.zoom = new_zoom;
        // new scroll keeps `doc` under `focal`: scroll = doc*zoom - focal
        let mut sx = doc.0 * new_zoom - focal.0;
        let mut sy = doc.1 * new_zoom - focal.1;
        // clamp scroll so the document stays sensibly on screen
        let doc_w = viewport.0; // caller supplies true doc size via clamp bounds below
        let _ = doc_w;
        sx = sx.max(-viewport.0);
        sy = sy.max(-viewport.1);
        self.scroll = (sx, sy);
    }

    /// Zooms by a multiplicative step around the viewport center.
    pub fn zoom_by(&mut self, factor: f64, viewport: (f64, f64)) {
        let focal = (viewport.0 / 2.0, viewport.1 / 2.0);
        self.set_zoom_focal(self.zoom * factor, focal, viewport);
    }
}

impl Default for DocumentZoomController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focal_zoom_keeps_document_point_stable() {
        let mut z = DocumentZoomController::new();
        z.set_scroll(100.0, 200.0);
        let focal = (300.0, 400.0);
        let viewport = (1000.0, 800.0);

        // document point under the focal point before zoom
        let doc_before = (
            (focal.0 + z.scroll().0) / z.zoom(),
            (focal.1 + z.scroll().1) / z.zoom(),
        );
        z.set_zoom_focal(2.0, focal, viewport);
        let doc_after = (
            (focal.0 + z.scroll().0) / z.zoom(),
            (focal.1 + z.scroll().1) / z.zoom(),
        );
        assert!((doc_before.0 - doc_after.0).abs() < 1e-9, "x point drifts");
        assert!((doc_before.1 - doc_after.1).abs() < 1e-9, "y point drifts");
        assert_eq!(z.zoom(), 2.0);
    }

    #[test]
    fn zoom_clamps() {
        let mut z = DocumentZoomController::new();
        z.set_zoom_focal(1000.0, (0.0, 0.0), (100.0, 100.0));
        assert_eq!(z.zoom(), ZOOM_MAX);
        z.set_zoom_focal(0.0001, (0.0, 0.0), (100.0, 100.0));
        assert_eq!(z.zoom(), ZOOM_MIN);
    }

    #[test]
    fn zoom_by_multiplies() {
        let mut z = DocumentZoomController::new();
        z.zoom_by(1.1, (800.0, 600.0));
        assert!((z.zoom() - 1.1).abs() < 1e-9);
    }
}
