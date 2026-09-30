//! Named atomic counters with snapshotting — the base layer of VDF metrics.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// A single monotonically increasing counter.
pub struct Counter {
    name: &'static str,
    value: AtomicU64,
}

impl Counter {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            value: AtomicU64::new(0),
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_by(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }
}

/// Registry of named counters. Registration is idempotent: re-registering a
/// name returns the existing counter.
#[derive(Default)]
pub struct MetricsRegistry {
    counters: Mutex<BTreeMap<&'static str, Arc<Counter>>>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, name: &'static str) -> Arc<Counter> {
        let mut map = self.counters.lock().unwrap();
        map.entry(name)
            .or_insert_with(|| Arc::new(Counter::new(name)))
            .clone()
    }

    /// Point-in-time snapshot of all counter values, sorted by name.
    pub fn snapshot(&self) -> BTreeMap<String, u64> {
        self.counters
            .lock()
            .unwrap()
            .iter()
            .map(|(name, c)| ((*name).to_string(), c.get()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_is_idempotent_and_counts() {
        let reg = MetricsRegistry::new();
        let c = reg.register("tiles.published");
        c.inc();
        c.inc_by(8);
        assert_eq!(c.get(), 9);

        // same name → same counter
        let c2 = reg.register("tiles.published");
        c2.inc();
        assert_eq!(c.get(), 10);

        let snap = reg.snapshot();
        assert_eq!(snap.get("tiles.published"), Some(&10));
        assert_eq!(snap.len(), 1);
    }

    #[test]
    fn counters_are_thread_safe() {
        let reg = MetricsRegistry::new();
        let c = reg.register("input.samples");
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let c = Arc::clone(&c);
                std::thread::spawn(move || {
                    for _ in 0..10_000 {
                        c.inc();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(c.get(), 40_000);
    }
}
