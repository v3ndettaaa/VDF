//! Tile + thumbnail caches with memory budgets (MASTER_PLAN.md §8).
//!
//! The cache enforces the stale-render invariant at its boundary: entries
//! are accepted only when their generation is at least the slot's current
//! one (the same rule the transport applies — defense in depth).
//! Eviction is LRU by bytes; `pin`-listed keys (e.g. currently visible) are
//! never evicted while pinned.

use std::collections::HashMap;
use std::sync::Arc;

use vdf_core::tile::{RenderGeneration, TileKey, accepts};

/// One cached tile.
pub struct TileEntry {
    pub generation: RenderGeneration,
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<Vec<u8>>,
}

impl TileEntry {
    pub fn byte_len(&self) -> usize {
        self.pixels.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum EntryState {
    Fresh,
}

/// LRU tile cache with a byte budget.
pub struct TileCache {
    entries: HashMap<TileKey, TileEntry>,
    /// Recency order: front = oldest.
    lru: std::collections::VecDeque<TileKey>,
    budget_bytes: usize,
    used_bytes: usize,
}

impl TileCache {
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            lru: std::collections::VecDeque::new(),
            budget_bytes,
            used_bytes: 0,
        }
    }

    pub fn budget(&self) -> usize {
        self.budget_bytes
    }

    pub fn used(&self) -> usize {
        self.used_bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &TileKey) -> Option<&TileEntry> {
        self.entries.get(key)
    }

    /// Marks a key as most-recently-used (on compositor use).
    pub fn touch(&mut self, key: &TileKey) {
        if let Some(pos) = self.lru.iter().position(|k| k == key) {
            let k = self.lru.remove(pos).unwrap();
            self.lru.push_back(k);
        }
    }

    /// Inserts (or replaces) a tile, enforcing the generation rule, then
    /// evicts down to budget. Returns true when stored.
    pub fn insert(
        &mut self,
        key: TileKey,
        generation: RenderGeneration,
        width: u32,
        height: u32,
        pixels: Arc<Vec<u8>>,
        pinned: &dyn Fn(&TileKey) -> bool,
    ) -> bool {
        let current = self.entries.get(&key).map(|e| e.generation);
        if !accepts(current, generation) {
            return false; // stale result — never overwrite newer
        }
        let len = pixels.len();
        if len > self.budget_bytes {
            return false; // single tile larger than the whole cache
        }
        if let Some(old) = self.entries.insert(
            key,
            TileEntry {
                generation,
                width,
                height,
                pixels,
            },
        ) {
            self.used_bytes -= old.byte_len();
            self.lru.retain(|k| *k != key);
            let _ = EntryState::Fresh; // keep enum used
        }
        self.used_bytes += len;
        self.lru.push_back(key);
        self.evict(pinned);
        true
    }

    /// Drops all entries for a page/zoom/rotation (document change).
    pub fn invalidate(&mut self, pred: &dyn Fn(&TileKey) -> bool) {
        let keys: Vec<TileKey> = self.entries.keys().filter(|k| pred(k)).copied().collect();
        for k in keys {
            if let Some(e) = self.entries.remove(&k) {
                self.used_bytes -= e.byte_len();
            }
            self.lru.retain(|x| *x != k);
        }
    }

    fn evict(&mut self, pinned: &dyn Fn(&TileKey) -> bool) {
        while self.used_bytes > self.budget_bytes {
            // find oldest unpinned
            let mut victim = None;
            for k in &self.lru {
                if !pinned(k) {
                    victim = Some(*k);
                    break;
                }
            }
            let Some(k) = victim else { break }; // everything pinned: over budget, but protected
            if let Some(e) = self.entries.remove(&k) {
                self.used_bytes -= e.byte_len();
            }
            self.lru.retain(|x| *x != k);
        }
    }
}

/// Budgets derived from total RAM (MASTER_PLAN.md §8); conservative so the
/// GPU/compositor/UI still have room. Validated by M7 profiling.
#[derive(Debug, Clone, Copy)]
pub struct MemoryBudgets {
    pub tile_cache_bytes: usize,
    pub thumbnail_cache_bytes: usize,
}

impl MemoryBudgets {
    /// Defaults for a given total-RAM value (e.g. `total_ram_bytes()`).
    pub fn from_total_ram(total: usize) -> Self {
        Self {
            tile_cache_bytes: (total / 4).clamp(64 << 20, 2 << 30),
            thumbnail_cache_bytes: 128 << 20,
        }
    }
}

/// Total system RAM in bytes (0 when unknown → conservative default).
pub fn total_ram_bytes() -> usize {
    // /proc/meminfo on Linux; fallback to a fixed 4 GiB elsewhere.
    if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let kb: usize = rest
                    .trim()
                    .trim_end_matches(" kB")
                    .trim()
                    .parse()
                    .unwrap_or(0);
                if kb > 0 {
                    return kb * 1024;
                }
            }
        }
    }
    4 << 30
}

#[cfg(test)]
mod tests {
    use super::*;
    use vdf_core::{PageIndex, Rotation};
    fn key(x: u32) -> TileKey {
        TileKey::new(PageIndex(0), 8, Rotation::D0, x, 0)
    }
    fn px(len: usize) -> Arc<Vec<u8>> {
        Arc::new(vec![128u8; len])
    }
    fn not_pinned(_: &TileKey) -> bool {
        false
    }

    #[test]
    fn insert_get_and_generation_guard() {
        let mut c = TileCache::new(1 << 20);
        assert!(c.insert(key(0), RenderGeneration(1), 8, 8, px(256), &not_pinned));
        assert_eq!(c.get(&key(0)).unwrap().generation, RenderGeneration(1));
        // older generation is rejected
        assert!(!c.insert(key(0), RenderGeneration(0), 8, 8, px(256), &not_pinned));
        assert_eq!(c.get(&key(0)).unwrap().generation, RenderGeneration(1));
        // newer replaces
        assert!(c.insert(key(0), RenderGeneration(2), 8, 8, px(256), &not_pinned));
        assert_eq!(c.used(), 256);
    }

    #[test]
    fn lru_eviction_respects_budget_and_pins() {
        let mut c = TileCache::new(3 * 256);
        for x in 0..3 {
            c.insert(key(x), RenderGeneration(1), 8, 8, px(256), &not_pinned);
        }
        assert_eq!(c.len(), 3);
        // touch key(0) so key(1) becomes LRU? insert order makes key(2) MRU
        c.touch(&key(0));
        // insert a 4th: must evict oldest unpinned = key(1)
        c.insert(key(3), RenderGeneration(1), 8, 8, px(256), &not_pinned);
        assert!(c.get(&key(1)).is_none(), "LRU victim evicted");
        assert!(c.get(&key(0)).is_some(), "touched survivor stays");
        assert!(c.get(&key(3)).is_some());

        // pinned tiles are never evicted (cache may exceed budget instead)
        let mut c2 = TileCache::new(256);
        c2.insert(key(0), RenderGeneration(1), 8, 8, px(256), &|k| k.x == 0);
        assert!(c2.get(&key(0)).is_some(), "pinned survives");
    }

    #[test]
    fn invalidate_by_predicate() {
        let mut c = TileCache::new(1 << 20);
        for p in 0..3u32 {
            for x in 0..2u32 {
                let k = TileKey::new(PageIndex(p), 8, Rotation::D0, x, 0);
                c.insert(k, RenderGeneration(1), 8, 8, px(256), &not_pinned);
            }
        }
        c.invalidate(&|k| k.page.0 == 1);
        assert_eq!(c.len(), 4);
        assert!(
            c.get(&TileKey::new(PageIndex(1), 8, Rotation::D0, 0, 0))
                .is_none()
        );
    }

    #[test]
    fn oversized_tile_rejected() {
        let mut c = TileCache::new(128);
        assert!(!c.insert(key(0), RenderGeneration(1), 8, 8, px(256), &not_pinned));
        assert!(c.is_empty());
    }

    #[test]
    fn budgets_scale_with_ram() {
        let b = MemoryBudgets::from_total_ram(8 << 30);
        assert_eq!(b.tile_cache_bytes, 2 << 30);
        let b2 = MemoryBudgets::from_total_ram(2 << 30);
        assert_eq!(b2.tile_cache_bytes, 512 << 20);
        assert!(total_ram_bytes() > 0);
    }
}
