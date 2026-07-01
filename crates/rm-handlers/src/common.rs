//! Helpers every resource handler shares. Today's set:
//! - [`AppState`] — shared axum `State`
//! - [`wrap_in_doc`] — HTML shell until G1 lands the master template
//!   (escapes the title — XSS guard, codex P1 on PR #10)
//! - [`record_id_to_u64`] — URL → render-kit u64 adapter for
//!   record-keyed resources (Issue)
//! - [`identifier_to_u64`] — same adapter for slug-keyed resources
//!   (Project) and any future resource whose URL key is a string
//! - [`html_escape`] — minimal `& < > " '` escape, no extra dep
//! - [`encode_path_segment`] — percent-encode a single URL path
//!   segment so slug-keyed resources tolerate `# ? /` in the key
//!   (codex P2 on PR #13)
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

/// HTML-escape the five attribute-/content-significant chars
/// (`& < > " '`). No external dep — askama isn't a direct dep of
/// rm-handlers (it's transitive via ogar-render-askama; pulling it
/// in here re-triggers the askama_axum macro-expansion issue we hit
/// in W0.1).
///
/// Used for any user-controlled string that lands in handler-built
/// HTML *outside* the askama kit's render path — most importantly
/// document titles ([`wrap_in_doc`]) and the pre-rendered
/// `headline_html` passed to `render_detail` (the kit treats that
/// parameter as raw HTML per the cell-template contract; user data
/// must be escaped before it goes in).
#[must_use]
pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Percent-encode a single URL path segment. Encodes everything
/// that isn't an RFC-3986 unreserved char (`A-Z a-z 0-9 - . _ ~`)
/// or the path-segment-safe `:` `@`.
///
/// Project identifiers + role names are arbitrary admin labels;
/// without encoding a `#`/`?`/`/` in the name breaks the URL
/// (codex P2 on PR #13). No external `percent-encoding` dep —
/// the rule is small.
#[must_use]
pub fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let safe = matches!(
            b,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b':' | b'@'
        );
        if safe {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(hex_nibble(b >> 4));
            out.push(hex_nibble(b & 0x0F));
        }
    }
    out
}

#[inline]
fn hex_nibble(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'A' + n - 10) as char,
        _ => unreachable!(),
    }
}

/// Redmine's global `#top-menu` — the persistent bar of cross-app links
/// rendered on every page. The link set is fixed, so it's a `const` the
/// formatter splices in verbatim (no per-request allocation beyond the
/// outer `format!`). Mirrors Redmine's top menu shape (Home + the global
/// resource lists); the `id`/`class` hooks match Redmine so G1's
/// stylesheet drops straight in over this skeleton.
const TOP_MENU: &str = concat!(
    r#"<div id="top-menu"><ul class="menu">"#,
    r#"<li><a class="home" href="/">Home</a></li>"#,
    r#"<li><a href="/projects">Projects</a></li>"#,
    r#"<li><a href="/issues">Issues</a></li>"#,
    r#"<li><a href="/time_entries">Spent time</a></li>"#,
    r#"<li><a href="/news">News</a></li>"#,
    r#"<li><a href="/wiki">Wiki</a></li>"#,
    r#"</ul></div>"#,
);

/// Wrap a fragment in the **master-layout skeleton** — an HTML5 document
/// carrying Redmine's persistent chrome: the global `#top-menu` nav, the
/// `#header` app title, and a `#main` > `#content` wrapper the fragment
/// lands in. G1 (Plan §5 GUI chrome) swaps this Rust-built shell for the
/// real `base.askama` master template + a stylesheet; the markup shape
/// (ids/classes) is already Redmine-compatible so that swap is cosmetic.
///
/// The `title` parameter is **HTML-escaped** — handlers pass
/// user-controlled strings (issue subjects, project names) and the
/// raw value can't reach the `<title>` element verbatim (codex P1
/// on PR #10, stored XSS via a subject like
/// `</title><script>alert(1)</script>`).
///
/// `body` is treated as already-rendered HTML — askama-emitted by
/// the kit's `render_list` / `render_detail`, which run their own
/// `escape = "html"` on data-derived strings — and the handler-built
/// chrome (`render_action_bar`, the filter/sort/pagination strips),
/// which escape their own user-derived inputs.
#[must_use]
pub fn wrap_in_doc(title: &str, body: &str) -> String {
    let escaped_title = html_escape(title);
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\
         <meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{escaped_title} · Redmine RS</title>\
         </head><body>{TOP_MENU}\
         <div id=\"header\"><h1><a href=\"/\">Redmine RS</a></h1></div>\
         <div id=\"main\"><div id=\"content\">{body}</div></div>\
         </body></html>"
    )
}

/// Render the validation-error block shown above a create/edit form — a
/// `role="alert"` list of messages. Empty string when there are no errors
/// (so a clean form renders no stray block).
///
/// Factored here once a **third** form module (News) joined Issue +
/// Project as a caller — the project's "three points form a line" rule
/// (Plan §1.6). Callers pass `&'static str` literals from their own
/// `validate`, so no user-controlled content reaches this HTML; the
/// messages are spliced verbatim.
#[must_use]
pub(crate) fn render_errors(errors: &[&str]) -> String {
    if errors.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(64 + errors.len() * 32);
    out.push_str(r#"<div class="form-errors" role="alert"><ul>"#);
    for e in errors {
        out.push_str("<li>");
        out.push_str(e);
        out.push_str("</li>");
    }
    out.push_str("</ul></div>");
    out
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
    fn wrap_in_doc_escapes_xss_in_title() {
        // Codex P1 on PR #10: a subject like `</title><script>...`
        // must not reach the document's <title> verbatim.
        let html = wrap_in_doc("</title><script>alert(1)</script>", "<p>body</p>");
        assert!(
            !html.contains("</title><script>"),
            "raw markup escaped past <title>: {html}"
        );
        assert!(
            html.contains("&lt;/title&gt;&lt;script&gt;alert(1)&lt;/script&gt;"),
            "expected fully-escaped title: {html}"
        );
    }

    #[test]
    fn wrap_in_doc_renders_global_top_menu_and_content_wrapper() {
        // Every page carries Redmine's persistent chrome: the global
        // top menu, the app header, and a #content wrapper the fragment
        // lands inside.
        let html = wrap_in_doc("Issues", "<p>body</p>");
        assert!(
            html.contains(r#"id="top-menu""#),
            "top menu missing:\n{html}"
        );
        assert!(
            html.contains(r#"<a class="home" href="/">Home</a>"#),
            "Home link missing:\n{html}"
        );
        assert!(
            html.contains(r#"href="/projects""#) && html.contains(r#"href="/issues""#),
            "global resource links missing:\n{html}"
        );
        assert!(
            html.contains(r#"id="content""#),
            "content wrapper missing:\n{html}"
        );
        // The fragment still renders inside the shell.
        assert!(html.contains("<p>body</p>"), "body not wrapped:\n{html}");
        // Viewport meta for the eventual responsive stylesheet.
        assert!(html.contains("viewport"), "viewport meta missing:\n{html}");
    }

    #[test]
    fn render_errors_is_empty_without_errors_and_lists_them_otherwise() {
        assert!(render_errors(&[]).is_empty(), "no block when no errors");
        let html = render_errors(&["Subject is required.", "Too long."]);
        assert!(html.contains(r#"class="form-errors""#), "{html}");
        assert!(html.contains(r#"role="alert""#), "{html}");
        assert!(html.contains("<li>Subject is required.</li>"), "{html}");
        assert!(html.contains("<li>Too long.</li>"), "{html}");
    }

    #[test]
    fn html_escape_covers_the_five_chars() {
        assert_eq!(html_escape("&<>\"'"), "&amp;&lt;&gt;&quot;&#39;");
        assert_eq!(html_escape("plain text"), "plain text");
    }

    #[test]
    fn encode_path_segment_keeps_unreserved_chars() {
        assert_eq!(
            encode_path_segment("my-proj.v1_test~ok"),
            "my-proj.v1_test~ok"
        );
    }

    #[test]
    fn encode_path_segment_percent_encodes_reserved() {
        // Slashes / hashes / questions would break URL parsing
        // (codex P2 on PR #13).
        assert_eq!(encode_path_segment("a/b"), "a%2Fb");
        assert_eq!(encode_path_segment("a#b"), "a%23b");
        assert_eq!(encode_path_segment("a?b"), "a%3Fb");
        assert_eq!(encode_path_segment("a b"), "a%20b");
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
        assert_ne!(a, b, "hash collision on two different ids");
    }

    #[test]
    fn identifier_to_u64_is_deterministic_and_distinguishes() {
        assert_eq!(identifier_to_u64("alpha"), identifier_to_u64("alpha"));
        assert_ne!(identifier_to_u64("alpha"), identifier_to_u64("beta"));
    }
}
