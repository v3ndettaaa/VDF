//! Rasterization boundary: producing the pixels for one tile.
//!
//! M1 provides the real `MupdfRasterizer` (via `vdf-pdf`). M0 ships
//! [`SyntheticRasterizer`], a deterministic fake whose output depends only on
//! the tile key — enough to build and test scheduling/caching logic with no
//! PDF engine present.

use std::sync::atomic::{AtomicU64, Ordering};

use vdf_core::tile::{RenderGeneration, TileKey};
use vdf_core::{DocHandle, Revision, VdfError, VdfResult};

use crate::transport::FinishedTile;

/// Produces pixels for a tile request.
pub trait Rasterizer: Send + Sync {
    /// Renders one tile. Implementations must be pure with respect to inputs
    /// (same request → same pixels) so rendering stays testable and cacheable.
    fn rasterize_tile(
        &self,
        doc: DocHandle,
        rev: Revision,
        key: &TileKey,
        width: u32,
        height: u32,
    ) -> VdfResult<FinishedTile>;
}

/// Deterministic synthetic rasterizer for tests. Color derives from the tile
/// key hash; generation derives from the document revision.
pub struct SyntheticRasterizer {
    rendered: AtomicU64,
}

impl SyntheticRasterizer {
    pub fn new() -> Self {
        Self {
            rendered: AtomicU64::new(0),
        }
    }

    pub fn rendered_count(&self) -> u64 {
        self.rendered.load(Ordering::Relaxed)
    }
}

impl Default for SyntheticRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

fn mix(h: u64, v: u64) -> u64 {
    (h ^ v).wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(23)
}

impl Rasterizer for SyntheticRasterizer {
    fn rasterize_tile(
        &self,
        doc: DocHandle,
        rev: Revision,
        key: &TileKey,
        width: u32,
        height: u32,
    ) -> VdfResult<FinishedTile> {
        if width == 0 || height == 0 {
            return Err(VdfError::Render("tile dimensions must be nonzero".into()));
        }
        let len = width as usize * height as usize * 4;
        if len > 256 * 1024 * 1024 {
            return Err(VdfError::Render(format!("tile too large: {len} bytes")));
        }
        let mut h = 0x85EB_CA6B_7791_65ADu64;
        h = mix(h, doc.0);
        h = mix(h, key.page.0 as u64);
        h = mix(h, key.zoom_step as u64);
        h = mix(h, key.rotation.quarter_turns() as u64);
        h = mix(h, key.x as u64);
        h = mix(h, key.y as u64);
        let r = (h & 0xFF) as u8;
        let g = (h >> 8 & 0xFF) as u8;
        let b = (h >> 16 & 0xFF) as u8;
        // one visible gradient row so seams would be detectable in tests
        let mut rgba = Vec::with_capacity(len);
        for y in 0..height {
            let shade = (y * 255 / height.max(1)) as u8;
            for _ in 0..width {
                rgba.extend_from_slice(&[r, g, b, 255 - shade / 4]);
            }
        }
        self.rendered.fetch_add(1, Ordering::Relaxed);
        Ok(FinishedTile {
            doc,
            key: *key,
            generation: RenderGeneration(rev),
            width,
            height,
            rgba: rgba.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vdf_core::{DocumentId, PageIndex, Rotation};

    #[test]
    fn deterministic_and_correctly_sized() {
        let r = SyntheticRasterizer::new();
        let doc = DocumentId(3);
        let key = TileKey::new(PageIndex(1), 4, Rotation::D0, 2, 5);
        let a = r.rasterize_tile(doc, 9, &key, 32, 16).unwrap();
        let b = r.rasterize_tile(doc, 9, &key, 32, 16).unwrap();
        assert_eq!(a.rgba, b.rgba, "same request must produce identical pixels");
        assert_eq!(a.byte_len(), 32 * 16 * 4);
        assert_eq!(a.generation, RenderGeneration(9));
        assert_eq!(r.rendered_count(), 2);
    }

    #[test]
    fn rejects_degenerate_sizes() {
        let r = SyntheticRasterizer::new();
        let key = TileKey::new(PageIndex(0), 0, Rotation::D0, 0, 0);
        assert!(r.rasterize_tile(DocumentId(1), 1, &key, 0, 8).is_err());
        assert!(r.rasterize_tile(DocumentId(1), 1, &key, 8, 0).is_err());
    }
}
