//! Recognizer architecture: the extension point for handwriting intelligence
//! (MASTER_PLAN.md §13). M4 implements shape recognition if it earns its
//! place; text/math/diagram recognizers remain future work. The trait exists
//! now so the ink model never has to be reshaped to accommodate them.

use vdf_core::{VdfError, VdfResult};

/// One sampled ink point handed to a recognizer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InkPoint {
    pub x: f32,
    pub y: f32,
    pub t_ms: f64,
}

/// A read-only view of ink strokes (document-space coordinates).
pub struct InkCanvas<'a> {
    /// Strokes, each a sequence of points.
    pub strokes: &'a [Vec<InkPoint>],
}

/// What kind of thing a recognizer thinks it found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecognitionKind {
    Text,
    Math,
    Shape,
    Diagram,
}

/// One candidate recognition of an ink canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct RecognitionCandidate {
    pub kind: RecognitionKind,
    /// Recognized label (text content, shape name, LaTeX, ...).
    pub label: String,
    /// 0..1 confidence; never fabricated — engines that cannot score
    /// honestly report a conservative constant they document.
    pub confidence: f32,
}

/// Turns ink into structured guesses. Implementations must be pure,
/// side-effect free, and runnable on background threads (OCR and
/// recognition must never block handwriting — MASTER_PLAN.md §13).
pub trait Recognizer: Send + Sync {
    fn recognize(&self, canvas: &InkCanvas<'_>) -> VdfResult<Vec<RecognitionCandidate>>;
}

/// Placeholder guard: M0 ships no recognizer, and none may be faked.
pub fn no_recognizers_registered() -> VdfError {
    VdfError::NotImplemented {
        milestone: "M4",
        what: "handwriting recognition engines".into(),
    }
}
