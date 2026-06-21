//! Store-side error type. One enum, one variant per failure mode.
//!
//! `rm-server`'s `AppError` already has an `Internal` catch-all for
//! handler boundaries; `StoreError` is the typed enum behind it so
//! handlers can pattern-match on store failures (e.g. `NotFound` →
//! 404 instead of 500).

use thiserror::Error;

/// Errors emitted by the store layer.
#[derive(Debug, Error)]
pub enum StoreError {
    /// The SurrealDB driver returned an error — connection, schema
    /// application, query failure, value conversion via the
    /// `SurrealValue` trait.
    #[error("surrealdb: {0}")]
    Surreal(#[from] surrealdb::Error),

    /// The record didn't exist.
    #[error("not found")]
    NotFound,
}
