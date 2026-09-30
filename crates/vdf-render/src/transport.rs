//! Rendering/surface transport abstraction (MASTER_PLAN.md §6, §10).
//!
//! M0: the trait plus an in-memory fake used by tests.
//! M1: a practical binary transport over a Tauri custom URI-scheme protocol.
//! M7: profile under heavy load; optimize or replace *behind this trait*.
//!
//! The trait embodies the stale-render invariant: a transport implementation
//! must only let a tile occupy a slot when its generation is at least the
//! slot's current expected generation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use vdf_core::tile::{RenderGeneration, TileKey, accepts};
use vdf_core::{DocHandle, Revision, VdfResult};

/// A finished, generation-tagged tile ready for transport to the compositor.
pub struct FinishedTile {
    pub doc: DocHandle,
    pub key: TileKey,
    pub generation: RenderGeneration,
    pub width: u32,
    pub height: u32,
    /// RGBA8, row-major, `width * height * 4` bytes.
    pub rgba: Arc<Vec<u8>>,
}

impl FinishedTile {
    pub fn byte_len(&self) -> usize {
        self.rgba.len()
    }
}

/// Publishes finished tiles to whatever displays them.
///
/// `publish_tile` must not block the scheduler; transports that need to hand
/// off to another thread should do so with bounded queues.
pub trait RenderTransport: Send + Sync {
    /// Makes a finished tile available. Late/stale tiles may be dropped by
    /// the transport (counted in diagnostics), which is correct behavior.
    fn publish_tile(&self, tile: FinishedTile) -> VdfResult<()>;

    /// Drops transport-side resources for a document revision (document
    /// closed, page tree changed, eviction cascade).
    fn invalidate(&self, doc: DocHandle, rev: Revision) -> VdfResult<()>;
}

/// Observable counters for a transport (diagnostics + tests).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportStats {
    pub published: u64,
    pub stale_dropped: u64,
    pub invalidated: u64,
    pub retained: u64,
}

/// In-memory transport used by tests and the M0 wiring. Enforces the
/// stale-drop rule exactly like the real M1 transport must.
#[derive(Default)]
pub struct MemoryTransport {
    state: Mutex<MemoryState>,
}

/// (doc, tile key) → (retained generation, pixels)
type TileSlotMap = HashMap<(DocHandle, TileKey), (RenderGeneration, Arc<Vec<u8>>)>;

#[derive(Default)]
struct MemoryState {
    tiles: TileSlotMap,
    stats: TransportStats,
}

impl MemoryTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stats(&self) -> TransportStats {
        self.state.lock().unwrap().stats
    }

    /// Generation currently retained for a slot, if any.
    pub fn generation_of(&self, doc: DocHandle, key: &TileKey) -> Option<RenderGeneration> {
        self.state
            .lock()
            .unwrap()
            .tiles
            .get(&(doc, *key))
            .map(|(g, _)| *g)
    }
}

impl RenderTransport for MemoryTransport {
    fn publish_tile(&self, tile: FinishedTile) -> VdfResult<()> {
        let mut st = self.state.lock().unwrap();
        let slot = (tile.doc, tile.key);
        let current = st.tiles.get(&slot).map(|(g, _)| *g);
        if accepts(current, tile.generation) {
            let len = tile.rgba.len();
            st.tiles.insert(slot, (tile.generation, tile.rgba));
            st.stats.published += 1;
            st.stats.retained = st.tiles.len() as u64;
            debug_assert_eq!(len, tile.width as usize * tile.height as usize * 4);
        } else {
            // a late result must never overwrite a newer one
            st.stats.stale_dropped += 1;
        }
        Ok(())
    }

    fn invalidate(&self, doc: DocHandle, _rev: Revision) -> VdfResult<()> {
        let mut st = self.state.lock().unwrap();
        let before = st.tiles.len();
        st.tiles.retain(|(d, _), _| *d != doc);
        st.stats.invalidated += (before - st.tiles.len()) as u64;
        st.stats.retained = st.tiles.len() as u64;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vdf_core::{DocumentId, PageIndex, Rotation};

    fn tile(doc: DocHandle, key: TileKey, generation: u64, w: u32, h: u32) -> FinishedTile {
        FinishedTile {
            doc,
            key,
            generation: RenderGeneration(generation),
            width: w,
            height: h,
            rgba: Arc::new(vec![128u8; (w * h * 4) as usize]),
        }
    }

    #[test]
    fn stale_generation_never_overwrites_newer() {
        let t = MemoryTransport::new();
        let doc = DocumentId(1);
        let key = TileKey::new(PageIndex(0), 8, Rotation::D0, 0, 0);

        // render A@100% (gen 1) and B@200% (gen 2) for the same slot;
        // B completes first, A lands later — A must be dropped.
        t.publish_tile(tile(doc, key, 2, 64, 64)).unwrap();
        t.publish_tile(tile(doc, key, 1, 64, 64)).unwrap();

        assert_eq!(t.generation_of(doc, &key), Some(RenderGeneration(2)));
        let s = t.stats();
        assert_eq!(s.published, 1);
        assert_eq!(
            s.stale_dropped, 1,
            "the late low-generation tile must be dropped"
        );
    }

    #[test]
    fn newer_generation_replaces_older() {
        let t = MemoryTransport::new();
        let doc = DocumentId(2);
        let key = TileKey::new(PageIndex(3), 16, Rotation::D90, 1, 2);
        t.publish_tile(tile(doc, key, 1, 32, 32)).unwrap();
        t.publish_tile(tile(doc, key, 5, 32, 32)).unwrap();
        assert_eq!(t.generation_of(doc, &key), Some(RenderGeneration(5)));
        assert_eq!(t.stats().stale_dropped, 0);
    }

    #[test]
    fn invalidate_scopes_to_document() {
        let t = MemoryTransport::new();
        let d1 = DocumentId(10);
        let d2 = DocumentId(20);
        let key = TileKey::new(PageIndex(0), 0, Rotation::D0, 0, 0);
        t.publish_tile(tile(d1, key, 1, 16, 16)).unwrap();
        t.publish_tile(tile(d2, key, 1, 16, 16)).unwrap();
        t.invalidate(d1, 7).unwrap();
        assert_eq!(t.generation_of(d1, &key), None);
        assert_eq!(t.generation_of(d2, &key), Some(RenderGeneration(1)));
        assert_eq!(t.stats().invalidated, 1);
    }
}
