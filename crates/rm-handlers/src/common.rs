//! Helpers every resource handler shares. Today's set:
//! - [`AppState`] — shared axum `State`
//! - [`wrap_in_doc`] — HTML shell until G1 lands the master template
//! - [`record_id_to_u64`] — URL → render-kit u64 adapter for
//!   record-keyed resources (Issue)
//! - [`identifier_to_u64`] — same adapter for slug-keyed resources
//!   (Project) and any future resource whose URL key is a string
//! - [`HandlerError`] — the per-resource error enum; factored out
//!   when W3 landed as the third caller (Plan §1.6 "three points
//!   form a line")

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rm_store::{Store, StoreError};
use surrealdb_types::{RecordId, ToSql};
use tracing::error;

/// State every resource handler gets via axum `State`. Cloneable —
/// inner pieces are `Arc`-shaped already (`Store` clones the
/// SurrealDB connection handle).
#[derive(Clone)]
pub struct AppState {
    /// The connected SurrealDB store.
    pub store: Store,
}

/// Wrap a fragment in a minimal HTML5 document. G1 (Plan §5 GUI
/// chrome) will replace this with the master `base.askama` template
/// — at that point this fn deletes and every handler uses
/// `askama_axum::Template` directly.
#[must_use]
pub fn wrap_in_doc(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\
         <meta charset=\"utf-8\">\
         <title>{title} · Redmine RS</title>\
         </head><body>{body}</body></html>"
    )
}

/// Hash a SurrealDB `RecordId` to a `u64` — the render kit's
/// `render_detail(record_id: u64, ...)` parameter takes an integer
/// for display + the `data-record-id` HTML attribute. SurrealDB
/// ULIDs are strings; we hash deterministically so the integer id
/// is stable across requests for the same row.
///
/// W4 / a later URL-conventions sprint can swap this for a real
/// integer key on the row (Redmine's `id INTEGER PRIMARY KEY`
/// shape) without changing the render-kit signature.
#[must_use]
pub fn record_id_to_u64(rid: &RecordId) -> u64 {
    let mut h = DefaultHasher::new();
    // `RecordId` doesn't impl `Display`, but it impls SurrealDB's
    // `ToSql`. `to_sql()` produces a stable string form (the
    // canonical `table:key` shape) — same input → same hash.
    rid.to_sql().hash(&mut h);
    h.finish()
}

/// Hash a string identifier (URL slug, e.g. a project's
/// `identifier`) to a `u64`. Distinct call from
/// [`record_id_to_u64`] because slug-keyed resources (Project) and
/// record-keyed ones (Issue) have different URL semantics; sharing
/// the impl through a single fn would be a footgun.
#[must_use]
pub fn identifier_to_u64(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Per-resource handler error. Factored out when W3 landed as the
/// third caller carrying an identical variant set across `issues.rs`,
/// `projects.rs`, `time_entries.rs` (Plan §1.6).
///
/// `Render` carries a `String`, not `askama::Error` — `askama` isn't
/// a direct dep of `rm-handlers` (it's transitive through
/// `ogar-render-askama`), and Rust doesn't let us name a transitive
/// extern crate's type without re-declaring it. The render kit's
/// `Result<String, askama::Error>` gets stringified at the call site.
#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
    /// The store returned an error.
    #[error("store: {0}")]
    Store(#[from] StoreError),
    /// Askama rendering failed; body carries the formatted error.
    #[error("render: {0}")]
    Render(String),
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Store(StoreError::NotFound) => StatusCode::NOT_FOUND,
            Self::Store(_) | Self::Render(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        error!(error = %self, "handler error");
        (status, status.canonical_reason().unwrap_or("error")).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_in_doc_includes_title_and_body() {
        let html = wrap_in_doc("Issues", "<p>hello</p>");
        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("<title>Issues · Redmine RS</title>"));
        assert!(html.contains("<p>hello</p>"));
    }

    #[test]
    fn record_id_to_u64_is_deterministic() {
        let rid = RecordId::new("project_work_item", "abc");
        let a = record_id_to_u64(&rid);
        let b = record_id_to_u64(&rid);
        assert_eq!(a, b, "hash must be deterministic across calls");
    }

    #[test]
    fn record_id_to_u64_distinguishes_different_ids() {
        let a = record_id_to_u64(&RecordId::new("project_work_item", "id_a"));
        let b = record_id_to_u64(&RecordId::new("project_work_item", "id_b"));
        // DefaultHasher isn't collision-proof but two different keys
        // landing on the same u64 is astronomically unlikely; if this
        // ever fails it's worth investigating.
        assert_ne!(a, b, "hash collision on two different ids");
    }

    #[test]
    fn identifier_to_u64_is_deterministic_and_distinguishes() {
        assert_eq!(identifier_to_u64("alpha"), identifier_to_u64("alpha"));
        assert_ne!(identifier_to_u64("alpha"), identifier_to_u64("beta"));
    }
}
