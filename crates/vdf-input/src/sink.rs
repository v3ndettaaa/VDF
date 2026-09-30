//! Where the input pipeline receives batches of samples.
//!
//! M2's transport (batched binary IPC, fire-and-forget, no per-sample ack —
//! MASTER_PLAN.md §11) delivers into this sink. Keeping the trait here means
//! `vdf-input` never learns the delivery mechanism.

use vdf_core::VdfResult;

use crate::sample::PointerSample;

/// Events the input engine consumes.
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// A batch of samples in arrival order (already time-ordered per pointer).
    Batch(Vec<PointerSample>),
}

/// Receives input events from whatever transport the app layer provides.
pub trait InputSink: Send {
    fn on_event(&self, event: InputEvent) -> VdfResult<()>;
}

/// The M0 no-op sink (plumbing until the gesture engine exists in M2).
pub struct NoopSink;

impl InputSink for NoopSink {
    fn on_event(&self, _event: InputEvent) -> vdf_core::VdfResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_sink_accepts_batches() {
        let sink = NoopSink;
        let batch = InputEvent::Batch(vec![PointerSample::mouse(1, 0.0, 0.0, 0.0)]);
        sink.on_event(batch).unwrap();
    }
}
