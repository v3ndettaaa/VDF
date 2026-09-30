//! vdf-input — input abstraction, gesture engine, stroke pipeline (arrives in M2).
//!
//! M0 establishes the sample and sink types only. The engine itself —
//! batching, gestures, prediction (display-only), smoothing, stabilization,
//! brushes — is M2 work. This crate must never depend on Tauri IPC details
//! or browser APIs; samples arrive through [`sink::InputSink`] however the
//! app layer chooses to deliver them (MASTER_PLAN.md §5, §11).

pub mod sample;
pub mod sink;

pub use sample::{PointerButtons, PointerSample, Tool};
pub use sink::{InputEvent, InputSink, NoopSink};
