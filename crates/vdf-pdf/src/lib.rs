//! vdf-pdf — MuPDF integration (arrives in M1).
//!
//! This crate is the *only* consumer of the `mupdf-sys` binding
//! (MASTER_PLAN.md §5). In M1 it will provide: document open/validate, page
//! tree + metadata access, region rasterization to RGBA, structured text
//! extraction, annotation read/write, and save. Until then this crate is an
//! intentionally empty placeholder — M0 must not parse or render PDFs.
//!
//! Nothing to see yet, by design.
