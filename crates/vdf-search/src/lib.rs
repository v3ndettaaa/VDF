//! vdf-search — search, indexing, OCR adapter, recognizers (M4).
//!
//! M0 delivers the pieces with no PDF dependency: the query model with real
//! match semantics (case sensitivity, whole-word, regex), and the
//! `Recognizer` trait that keeps handwriting intelligence architecturally
//! possible without faking it.
//!
//! M4 adds: background per-page indexing over extracted text, Unicode
//! normalization (NFKC + Arabic/Persian handling) with offset→rect maps,
//! replace via the command system, and the Tesseract-backed `OcrEngine`.

pub mod query;
pub mod recognizer;

pub use query::{SearchQuery, normalize_for_search};
pub use recognizer::{InkCanvas, InkPoint, RecognitionCandidate, RecognitionKind, Recognizer};
