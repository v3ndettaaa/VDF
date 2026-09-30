//! Persistent identity types.
//!
//! Rules (MASTER_PLAN.md §7):
//! - array indices are never identity
//! - IDs are never reused within a document (deletion creates tombstones so
//!   undo can restore)
//! - IDs serialize as `"o12"` / `"p3"` style strings in text formats

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// Identity of a document instance (one open file = one `DocumentId`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentId(pub u64);

impl fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "d{}", self.0)
    }
}

/// Alias used at transport boundaries (e.g. `RenderTransport::invalidate`).
pub type DocHandle = DocumentId;

/// Monotonic revision counter for a document; bumped on every applied command.
pub type Revision = u64;

/// Identity of an object (ink stroke, shape, text box, image, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId(pub u64);

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "o{}", self.0)
    }
}

/// Identity of a page within a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PageId(pub u64);

impl fmt::Display for PageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "p{}", self.0)
    }
}

/// Thread-safe generator for per-document IDs.
///
/// The epoch makes IDs from different document instances practically
/// non-colliding without needing a UUID dependency; the counter makes IDs
/// strictly increasing within a document, which keeps serialization and
/// debug output stable.
pub struct IdGenerator {
    epoch: u64,
    next: AtomicU64,
}

impl IdGenerator {
    /// Creates a generator seeded from a wall-clock epoch and starts the
    /// counter at 1 (0 is reserved as "no id").
    pub fn new() -> Self {
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Self {
            epoch,
            next: AtomicU64::new(1),
        }
    }

    /// Creates a generator with an explicit epoch (tests, deserialization).
    pub fn with_epoch(epoch: u64) -> Self {
        Self {
            epoch,
            next: AtomicU64::new(1),
        }
    }

    pub fn document_id(&self) -> DocumentId {
        DocumentId(
            self.epoch
                ^ self
                    .next
                    .load(Ordering::Relaxed)
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15),
        )
    }

    pub fn next_object_id(&self) -> ObjectId {
        ObjectId(self.next.fetch_add(1, Ordering::Relaxed))
    }

    pub fn next_page_id(&self) -> PageId {
        PageId(self.next.fetch_add(1, Ordering::Relaxed))
    }

    /// The value the next generated id will carry.
    pub fn peek(&self) -> u64 {
        self.next.load(Ordering::Relaxed)
    }
}

impl Default for IdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_strictly_increasing() {
        let g = IdGenerator::with_epoch(1);
        let a = g.next_object_id();
        let b = g.next_object_id();
        let c = g.next_object_id();
        assert!(a.0 < b.0 && b.0 < c.0, "ids must strictly increase");
        assert_eq!(a, ObjectId(1));
    }

    #[test]
    fn ids_do_not_collide_across_threads() {
        let g = std::sync::Arc::new(IdGenerator::with_epoch(7));
        let mut seen: HashSet<ObjectId> = HashSet::new();
        let mut handles = Vec::new();
        for _ in 0..4 {
            let g = std::sync::Arc::clone(&g);
            handles.push(std::thread::spawn(move || {
                (0..1000).map(|_| g.next_object_id()).collect::<Vec<_>>()
            }));
        }
        for h in handles {
            for id in h.join().unwrap() {
                assert!(
                    seen.insert(id),
                    "duplicate id {id} — ids must never be reused"
                );
            }
        }
        assert_eq!(seen.len(), 4000);
    }

    #[test]
    fn display_format_is_stable() {
        assert_eq!(ObjectId(12).to_string(), "o12");
        assert_eq!(PageId(3).to_string(), "p3");
        assert_eq!(DocumentId(99).to_string(), "d99");
    }
}
