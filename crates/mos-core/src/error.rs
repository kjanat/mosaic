//! The crate-level error type and `Result` alias.
//!
//! [`CoreError`] is a convenience error for crates that want a single
//! `Result` without inventing their own; it wraps a [`Diagnostic`] or an
//! `Unimplemented` marker.

use crate::Diagnostic;

/// Convenience top-level error type for crates that want a single
/// `Result` alias without inventing their own.
///
/// # Examples
///
/// ```
/// use mos_core::CoreError;
///
/// let err = CoreError::Unimplemented("cache");
///
/// assert_eq!(err.to_string(), "not yet implemented: cache");
/// ```
#[derive(thiserror::Error, Debug)]
pub enum CoreError {
    #[error("not yet implemented: {0}")]
    Unimplemented(&'static str),

    #[error(transparent)]
    Diagnostic(Box<Diagnostic>),
}

pub type Result<T> = std::result::Result<T, CoreError>;
