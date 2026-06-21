//! **W1** — Issue (`project_work_item`) list + detail handlers.
//!
//! The hot-path resource: Redmine's `Issue`, OpenProject's
//! `WorkPackage`, both routing through the canonical
//! `class_ids::PROJECT_WORK_ITEM` (`0x0102`) arm.
//!
//! Routes (mounted by [`router`] at the workspace router):
//!
//! - `GET /issues` — list page, columns `#` + `Subject`
//! - `GET /issues/:id` — detail page for a single issue
//!
//! # Today's scope
//!
//! - Columns hardcoded; the `default_columns_for(&class)` factoring
//!   lands when W2 (Project) introduces a second caller.
//! - No filter / sort / group on the list (D2 — Plan §4).
//! - No edit form (D1 — Plan §4); the create form already exists in
//!   `rm-handlers/src/issues_form.rs` (TODO, follow-up).
//! - URL `:id` is the SurrealDB record-key segment (string ulid).
//!   The render-kit's u64 `record_id` parameter is filled via
//!   [`common::record_id_to_u64`] (stable hash); proper integer ids
//!   land when a Redmine-shaped `iid` column is added to the row.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use ogar_render_askama::{
    render_detail, render_list, CellData, CellSource, ColumnKind, RenderColumn, RowSource,
};
use rm_store::{IssueRow, StoreError};
use surrealdb_types::{RecordId, ToSql};
use tracing::error;

use crate::common::{record_id_to_u64, wrap_in_doc, AppState};

/// `GET /issues` — render the issue list.
pub async fn list(State(state): State<AppState>) -> Result<Html<String>, HandlerError> {
    let issues = state.store.list_issues().await?;
    let cols = list_columns();
    // Row construction is allocation-light: each row keeps refs to
    // the issue's strings + the column slice. The askama kit takes
    // a `&[RowSource<'_>]` so the lifetime ties to `issues` + `cols`,
    // both held in this scope.
    let hrefs: Vec<String> = issues
        .iter()
        .map(|i| {
            i.id.as_ref()
                .map(|rid| format!("/issues/{}", rid.key.to_sql()))
                .unwrap_or_default()
        })
        .collect();
    let ids: Vec<u64> = issues
        .iter()
        .map(|i| i.id.as_ref().map(record_id_to_u64).unwrap_or(0))
        .collect();
    let rows: Vec<RowSource<'_>> = issues
        .iter()
        .enumerate()
        .map(|(idx, issue)| RowSource {
            record_id: ids[idx],
            css_classes: "issue",
            group: None,
            inline: vec![
                CellSource {
                    column: &cols[0],
                    css_classes: "num",
                    data: CellData::IdLink {
                        id: ids[idx],
                        href: &hrefs[idx],
                    },
                },
                CellSource {
                    column: &cols[1],
                    css_classes: "",
                    data: CellData::PrimaryLink {
                        label: &issue.subject,
                        href: &hrefs[idx],
                    },
                },
            ],
            block: Vec::new(),
        })
        .collect();
    let body = render_list("Issues", 0x0102, "project_work_item", &cols, &[], &rows)
        .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc("Issues", &body)))
}

/// `GET /issues/:id` — render an issue's detail page.
pub async fn detail(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Html<String>, HandlerError> {
    let rid = RecordId::new("project_work_item", id_str.as_str());
    let issue: IssueRow = state.store.find_issue(&rid).await?;
    let cols = detail_columns();
    let href = format!("/issues/{}", id_str);
    let headline = format!(
        "<a href=\"{}\" class=\"primary-link\">{}</a>",
        href, &issue.subject
    );
    let subtitle = issue.description.as_deref().unwrap_or("");
    let cells = vec![CellSource {
        column: &cols[0],
        css_classes: "",
        data: CellData::PrimaryLink {
            label: &issue.subject,
            href: &href,
        },
    }];
    let body = render_detail(
        0x0102,
        "project_work_item",
        record_id_to_u64(&rid),
        &headline,
        subtitle,
        &cols,
        &cells,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc(
        &format!("#{} {}", rid.key.to_sql(), &issue.subject),
        &body,
    )))
}

/// The Issue list-view columns. Hardcoded for W1; factors into a
/// `default_columns_for(&class)` helper when W2 lands and two
/// resources need the same default-column shape.
fn list_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("id", "#", ColumnKind::IdLink)
            .sortable()
            .frozen(),
        RenderColumn::new("subject", "Subject", ColumnKind::PrimaryLink).sortable(),
    ]
}

/// The Issue detail-view columns. Same lifecycle as `list_columns()`.
fn detail_columns() -> [RenderColumn; 1] {
    [RenderColumn::new(
        "subject",
        "Subject",
        ColumnKind::PrimaryLink,
    )]
}

/// Build the Issue router. `rm-server` merges this in via
/// `.merge(rm_handlers::issues::router(state))` — Plan §8 file
/// ownership: rm-server's `routes.rs` is the only shared file in
/// the W1..W8 fan-out.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/issues", get(list))
        .route("/issues/:id", get(detail))
        .with_state(state)
}

/// Handler error type. Maps to HTTP status; logs the underlying
/// detail before mapping so the body never leaks more than a status
/// line.
///
/// `Render` carries a `String`, not `askama::Error` — `askama` isn't a
/// direct dep of `rm-handlers` (it's transitive through
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
        error!(error = %self, "issue handler error");
        (status, status.canonical_reason().unwrap_or("error")).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use rm_store::{NewIssue, Store};
    use tower::ServiceExt;

    async fn app_with_issues(seed: &[(&str, Option<&str>)]) -> Router {
        let store = Store::open().await.expect("store boots");
        for (subject, desc) in seed {
            store
                .create_issue(NewIssue {
                    subject: subject.to_string(),
                    description: desc.map(|s| s.to_string()),
                })
                .await
                .expect("seed insert");
        }
        router(AppState { store })
    }

    #[tokio::test]
    async fn list_renders_empty_state_when_no_issues() {
        let app = app_with_issues(&[]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/issues")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(
            s.contains("data-class-id=\"0x0102\""),
            "expected canonical class id in:\n{s}"
        );
        assert!(s.contains("No data."), "expected empty-state in:\n{s}");
    }

    #[tokio::test]
    async fn list_renders_seeded_issues() {
        let app =
            app_with_issues(&[("Fix the foo", Some("a description")), ("Bar broken", None)]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/issues")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("Fix the foo"), "expected first subject in:\n{s}");
        assert!(s.contains("Bar broken"), "expected second subject in:\n{s}");
        // The IdLink column emits `/issues/<key>` hrefs.
        assert!(
            s.contains("href=\"/issues/"),
            "expected detail hrefs in:\n{s}"
        );
    }

    #[tokio::test]
    async fn detail_renders_a_known_issue() {
        // We need an actual record id; seed one and grab its key.
        let store = Store::open().await.unwrap();
        let inserted = store
            .create_issue(NewIssue {
                subject: "Detail target".to_string(),
                description: Some("body here".to_string()),
            })
            .await
            .unwrap();
        let rid = inserted.id.expect("inserted row carries an id");
        let key = rid.key.to_sql().to_string();

        let app = router(AppState { store });
        let res = app
            .oneshot(
                Request::builder()
                    .uri(format!("/issues/{key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("Detail target"), "expected subject in:\n{s}");
        assert!(
            s.contains("data-class-id=\"0x0102\""),
            "expected canonical class id in:\n{s}"
        );
    }

    #[tokio::test]
    async fn detail_returns_404_for_unknown_issue() {
        let app = app_with_issues(&[]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/issues/does_not_exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
