//! Shared handler error type. One enum, one `IntoResponse` impl —
//! every handler returns `Result<_, AppError>` and the response status
//! falls out of the variant.
//!
//! Per the [Integration Plan §9][plan] calibration gates, the round-trip
//! parse + target-toolchain-compile gates ride on `AppError` being a
//! pure-data enum (no dyn boxes); the `IntoResponse` impl stays
//! deterministic.
//!
//! [plan]: https://github.com/AdaWorldAPI/OGAR/blob/main/docs/integration/REDMINE-INTEGRATION-PLAN.md

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// One enum, one place — every handler error funnels through here.
///
/// New variants add a `StatusCode` arm; the kit doesn't need a
/// `dyn Error` escape hatch because handlers stay narrow (Plan §4
/// depth tracks define their own variant if they need a new HTTP
/// status — e.g. D3 RBAC's `Forbidden`).
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The requested record / route does not exist.
    #[error("not found")]
    NotFound,

    /// Template rendering failed — askama produced an error. Surfaces
    /// as 500 with the template name in the log line so the failing
    /// template is easy to locate. We never echo the template body to
    /// the client.
    #[error("template render: {0}")]
    Render(#[from] askama::Error),

    /// Catch-all for downstream errors that aren't expected at runtime.
    /// Adding a `StatusCode` arm is the right move when a new error
    /// stream gets recurring; don't grow this variant.
    #[error("internal: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Render(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        // Log every error before mapping it to a status — the body
        // never leaks more than a status line.
        tracing::error!(error = %self, "handler error");
        (status, status.canonical_reason().unwrap_or("error")).into_response()
    }
}
