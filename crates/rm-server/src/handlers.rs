//! Built-in handlers — the proof-of-shape `index` + health probe.
//!
//! Resource handlers (W1..W8 in the Integration Plan) live in a
//! sibling `rm-handlers` crate that this scaffolding leaves room for.
//! Each width track owns one file under `rm-handlers/src/<resource>.rs`
//! and calls [`crate::router::build_router::merge_resource`] to
//! register its routes. The fan-out is parallel after this W0.1 lands.

use axum::http::StatusCode;
use axum::response::Html;
use ogar_render_askama::{render, ArtifactKind};

use crate::AppError;

/// `GET /` — minimal end-to-end proof that the askama-driven render kit
/// reaches the browser via axum. Renders an empty `HtmlListView` for
/// the canonical `project_work_item` concept (Redmine's `Issue`,
/// OpenProject's `WorkPackage`).
///
/// Today the response wraps the kit output in a minimal HTML doc inline
/// so the round-trip is end-to-end-visible without depending on G1's
/// layout chrome. When G1 lands, this handler swaps the inline doc for
/// the `base.askama` master template.
pub async fn index() -> Result<Html<String>, AppError> {
    let class = ogar_vocab::project_work_item();
    let body = render(&class, ArtifactKind::HtmlListView)?;
    Ok(Html(format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\
         <meta charset=\"utf-8\">\
         <title>Redmine RS</title>\
         </head><body>{body}</body></html>",
    )))
}

/// `GET /healthz` — load-balancer probe. Returns `200 OK` with the
/// fixed body `"ok"`. Deliberately tiny so the probe stays cheap; the
/// server is healthy as long as it can serve any response at all.
pub async fn healthz() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn healthz_returns_ok() {
        let (status, body) = healthz().await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "ok");
    }

    #[tokio::test]
    async fn index_renders_canonical_class_id_in_html() {
        // The headline gate: the index page reaches the canonical
        // `project_work_item` arm through the askama kit, so the
        // emitted HTML carries the codebook id.
        let Html(body) = index().await.expect("index must render");
        assert!(
            body.contains("data-class-id=\"0x0102\""),
            "expected class_id 0x0102 (project_work_item) in:\n{body}"
        );
        assert!(
            body.contains("data-concept=\"project_work_item\""),
            "expected canonical concept in:\n{body}"
        );
        // The render kit's empty-state marker shows up when no rows
        // are supplied — confirms the kit is producing real HTML.
        assert!(
            body.contains("No data."),
            "expected empty-state marker in:\n{body}"
        );
    }
}
