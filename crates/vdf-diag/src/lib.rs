//! vdf-diag — diagnostics and performance metrics.
//!
//! M0 delivers the metrics registry (atomic counters, snapshotting) that the
//! render scheduler, input pipeline, and benchmarks will feed from M1 on.
//! Structured logging (`tracing` to rotating files) and the crash handler
//! land with the subsystems that produce the events (M1+).
//!
//! Hard rule: diagnostics never contain document content or credentials;
//! file references are hashed basenames only (MASTER_PLAN.md §13).

pub mod metrics;

pub use metrics::{Counter, MetricsRegistry};
