//! Page layout: where pages sit in view space for each page mode.
//!
//! View space is device pixels, y-down, origin at the document top-left.
//! A page's view size = its page-space size (pt, y-down) × zoom × dpr,
//! rotated per the view rotation. Layout is pure math — no PDF access.

use vdf_core::{PageIndex, Rect, Rotation};

/// Page layout modes (MASTER_PLAN.md §22).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageMode {
    /// One page below the other (default).
    Continuous,
    /// One page at a time.
    Single,
    /// Two pages side by side (odd page left).
    TwoPage,
}

/// Gap between pages / around the document, in device px.
pub const PAGE_GAP_PX: f64 = 12.0;

/// One page's rectangle in view space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRect {
    pub index: PageIndex,
    pub rect: Rect,
}

/// The full layout of a document at a given zoom/rotation/dpr.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentLayout {
    pub mode: PageMode,
    pub rotation: Rotation,
    /// Per-page rects in view space (device px), in page order.
    pub pages: Vec<PageRect>,
    /// Total document size in view space (device px).
    pub doc_size: (f64, f64),
}

/// View-space size of one page (pt size × zoom × dpr, rotated).
pub fn page_view_size(w_pt: f64, h_pt: f64, zoom: f64, rot: Rotation, dpr: f64) -> (f64, f64) {
    let (w, h) = match rot {
        Rotation::D0 | Rotation::D180 => (w_pt, h_pt),
        Rotation::D90 | Rotation::D270 => (h_pt, w_pt),
    };
    ((w * zoom * dpr).max(1.0), (h * zoom * dpr).max(1.0))
}

/// Computes the document layout. `page_sizes` must be the page-space (pt,
/// y-down) size for every page.
pub fn compute_layout(
    page_sizes: &[(f64, f64)],
    mode: PageMode,
    zoom: f64,
    rot: Rotation,
    dpr: f64,
) -> DocumentLayout {
    let mut pages = Vec::with_capacity(page_sizes.len());
    let doc_w;
    let doc_h;

    match mode {
        PageMode::Continuous => {
            let sized: Vec<(f64, f64)> = page_sizes
                .iter()
                .map(|&(w_pt, h_pt)| page_view_size(w_pt, h_pt, zoom, rot, dpr))
                .collect();
            doc_w = sized.iter().fold(0.0f64, |acc: f64, &(w, _)| acc.max(w));
            let mut y = PAGE_GAP_PX;
            for (i, &(w, h)) in sized.iter().enumerate() {
                let x = ((doc_w - w) / 2.0).max(0.0);
                pages.push(PageRect {
                    index: PageIndex(i as u32),
                    rect: Rect {
                        min: vdf_core::Point::new(x, y),
                        max: vdf_core::Point::new(x + w, y + h),
                    },
                });
                y += h + PAGE_GAP_PX;
            }
            doc_h = y;
        }
        PageMode::Single => {
            let mut max_w: f64 = 0.0;
            let mut max_h: f64 = 0.0;
            for &(w_pt, h_pt) in page_sizes {
                let (w, h) = page_view_size(w_pt, h_pt, zoom, rot, dpr);
                max_w = max_w.max(w);
                max_h = max_h.max(h);
            }
            doc_w = max_w + PAGE_GAP_PX * 2.0;
            doc_h = max_h + PAGE_GAP_PX * 2.0;
            for (i, _) in page_sizes.iter().enumerate() {
                // all pages occupy the same slot; visibility picks one
                pages.push(PageRect {
                    index: PageIndex(i as u32),
                    rect: Rect::from_size(max_w, max_h).placed(PAGE_GAP_PX, PAGE_GAP_PX),
                });
            }
        }
        PageMode::TwoPage => {
            let mut max_w: f64 = 0.0;
            let mut max_h: f64 = 0.0;
            for &(w_pt, h_pt) in page_sizes {
                let (w, h) = page_view_size(w_pt, h_pt, zoom, rot, dpr);
                max_w = max_w.max(w);
                max_h = max_h.max(h);
            }
            doc_w = max_w * 2.0 + PAGE_GAP_PX * 3.0;
            doc_h = max_h + PAGE_GAP_PX * 2.0;
            for i in 0..page_sizes.len() {
                let col = i % 2; // even index left, odd right
                let row = i / 2;
                let x = PAGE_GAP_PX + col as f64 * (max_w + PAGE_GAP_PX);
                let y = PAGE_GAP_PX + row as f64 * (max_h + PAGE_GAP_PX);
                pages.push(PageRect {
                    index: PageIndex(i as u32),
                    rect: Rect::from_size(max_w, max_h).placed(x, y),
                });
            }
        }
    }

    DocumentLayout {
        mode,
        rotation: rot,
        pages,
        doc_size: (doc_w, doc_h),
    }
}

/// Rect helper: translate a size-rect to (x, y), keeping its width/height.
trait Placed {
    fn placed(self, x: f64, y: f64) -> Rect;
}
impl Placed for Rect {
    fn placed(self, x: f64, y: f64) -> Rect {
        let w = self.width();
        let h = self.height();
        Rect {
            min: vdf_core::Point::new(x, y),
            max: vdf_core::Point::new(x + w, y + h),
        }
    }
}

impl DocumentLayout {
    /// Pages intersecting a view-space rect.
    pub fn pages_intersecting(&self, view: Rect) -> impl Iterator<Item = &PageRect> {
        self.pages.iter().filter(move |p| p.rect.intersects(view))
    }

    /// The page containing a view point, if any.
    pub fn page_at(&self, x: f64, y: f64) -> Option<PageIndex> {
        self.pages
            .iter()
            .find(|p| p.rect.contains(vdf_core::Point::new(x, y)))
            .map(|p| p.index)
    }

    /// View rect of a page index.
    pub fn rect_of(&self, index: PageIndex) -> Option<Rect> {
        self.pages.iter().find(|p| p.index == index).map(|p| p.rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vdf_core::Point;

    const A4: (f64, f64) = (595.0, 842.0);

    #[test]
    fn continuous_stacks_and_centers() {
        let sizes = vec![A4; 3];
        let layout = compute_layout(&sizes, PageMode::Continuous, 1.0, Rotation::D0, 1.0);
        assert_eq!(layout.pages.len(), 3);
        let r0 = layout.rect_of(PageIndex(0)).unwrap();
        let r1 = layout.rect_of(PageIndex(1)).unwrap();
        assert!((r0.width() - A4.0).abs() < 1e-9);
        assert!(
            (r1.min.y - (r0.max.y + PAGE_GAP_PX)).abs() < 1e-9,
            "pages stack with a gap"
        );
        assert!((r0.min.x - r1.min.x).abs() < 1e-9, "centered pages align");
        assert!(layout.doc_size.1 > r1.max.y);
    }

    #[test]
    fn single_mode_overlaps_pages() {
        let sizes = vec![A4; 3];
        let layout = compute_layout(&sizes, PageMode::Single, 1.0, Rotation::D0, 1.0);
        let r0 = layout.rect_of(PageIndex(0)).unwrap();
        let r1 = layout.rect_of(PageIndex(1)).unwrap();
        assert_eq!(r0, r1, "single mode: all pages share one slot");
    }

    #[test]
    fn two_page_places_pairs() {
        let sizes = vec![A4; 4];
        let layout = compute_layout(&sizes, PageMode::TwoPage, 1.0, Rotation::D0, 1.0);
        let r0 = layout.rect_of(PageIndex(0)).unwrap();
        let r1 = layout.rect_of(PageIndex(1)).unwrap();
        assert!(r1.min.x > r0.max.x, "page 1 sits right of page 0");
        let r2 = layout.rect_of(PageIndex(2)).unwrap();
        assert!(
            (r2.min.y - r0.min.y).abs() > 1.0,
            "page 2 is on the next row"
        );
    }

    #[test]
    fn rotation_swaps_view_size() {
        let (w, h) = page_view_size(A4.0, A4.1, 2.0, Rotation::D90, 1.0);
        assert!((w - A4.1 * 2.0).abs() < 1e-9 && (h - A4.0 * 2.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_scales_layout() {
        let sizes = vec![A4; 2];
        let a = compute_layout(&sizes, PageMode::Continuous, 1.0, Rotation::D0, 1.0);
        let b = compute_layout(&sizes, PageMode::Continuous, 2.0, Rotation::D0, 1.0);
        let (aw, _ah) = page_view_size(A4.0, A4.1, 1.0, Rotation::D0, 1.0);
        let (bw, _bh) = page_view_size(A4.0, A4.1, 2.0, Rotation::D0, 1.0);
        assert!((bw / aw - 2.0).abs() < 1e-9);
        assert!(b.doc_size.0 > a.doc_size.0);
    }

    #[test]
    fn page_at_and_intersection() {
        let sizes = vec![A4; 3];
        let layout = compute_layout(&sizes, PageMode::Continuous, 1.0, Rotation::D0, 1.0);
        let r1 = layout.rect_of(PageIndex(1)).unwrap();
        let c = r1.center();
        assert_eq!(layout.page_at(c.x, c.y), Some(PageIndex(1)));
        let mid_band = Rect {
            min: Point::new(0.0, c.y - 5.0),
            max: Point::new(100.0, c.y + 5.0),
        };
        let hit: Vec<_> = layout
            .pages_intersecting(mid_band)
            .map(|p| p.index)
            .collect();
        assert_eq!(hit, vec![PageIndex(1)]);
    }
}
