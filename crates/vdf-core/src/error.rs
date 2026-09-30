//! Common typed error for all VDF crates.

/// The single error type crossing crate boundaries in the VDF core.
///
/// Crates may wrap their own richer error types inside [`VdfError::Document`],
/// [`VdfError::Render`], etc. via `to_string()` at the boundary; inner detail
/// is logged, not propagated as nested enums, to keep the boundary stable.
#[derive(Debug, thiserror::Error)]
pub enum VdfError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("geometry error: {0}")]
    Geometry(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("document error: {0}")]
    Document(String),

    #[error("render error: {0}")]
    Render(String),

    #[error("input error: {0}")]
    Input(String),

    #[error("persistence error: {0}")]
    Persist(String),

    #[error("search error: {0}")]
    Search(String),

    /// For APIs whose contract exists but whose implementation is scheduled
    /// for a later milestone. Carrying the milestone name keeps "not yet"
    /// honest and greppable — it must never be used to fake functionality.
    #[error("not implemented yet (planned for {milestone}): {what}")]
    NotImplemented {
        milestone: &'static str,
        what: String,
    },

    #[error("internal error: {0}")]
    Internal(String),
}

pub type VdfResult<T> = Result<T, VdfError>;
