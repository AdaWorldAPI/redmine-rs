//! **W3** — TimeEntry (`billable_work_entry` codebook id `0x0103`)
//! list + detail handlers. Hours booked against a project / work item.
//!
//! Routes (mounted by [`router`]):
//!
//! - `GET /time_entries` — list page, columns `Date` + `Hours` + `Comments`
//! - `GET /time_entries/:id` — detail page (URL `:id` is the SurrealDB
//!   record-key segment, like W1's Issue)
//!
//! # Per Plan §1.6 ("three points form a line")
//!
//! W3 lands as the third caller of the per-resource scaffolding —
//! `HandlerError`, `record_id_to_u64`, `wrap_in_doc`, the cell-build
//! mechanics. `HandlerError` factored out into [`crate::common`]
//! when this module landed; the cell-build inline code stays for now
//! because each resource paints a different cell set (W1 = #/Subject,
//! W2 = Name/Identifier, W3 = Date/Hours/Comments).

use axum::extract::{Path, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use ogar_render_askama::{
    render_detail, render_list, CellData, CellSource, ColumnKind, RenderColumn, RowSource,
};
use rm_store::TimeEntryRow;
use surrealdb_types::{RecordId, ToSql};

use crate::common::{record_id_to_u64, wrap_in_doc, AppState, HandlerError};

/// `GET /time_entries` — render the list of booked time entries.
pub async fn list(State(state): State<AppState>) -> Result<Html<String>, HandlerError> {
    let entries = state.store.list_time_entries().await?;
    let cols = list_columns();
    let hrefs: Vec<String> = entries
        .iter()
        .map(|e| {
            e.id.as_ref()
                .map(|rid| format!("/time_entries/{}", rid.key.to_sql()))
                .unwrap_or_default()
        })
        .collect();
    let ids: Vec<u64> = entries
        .iter()
        .map(|e| e.id.as_ref().map(record_id_to_u64).unwrap_or(0))
        .collect();
    let hours_strs: Vec<String> = entries.iter().map(|e| format!("{:.2}", e.hours)).collect();
    let comments_strs: Vec<&str> = entries
        .iter()
        .map(|e| e.comments.as_deref().unwrap_or(""))
        .collect();
    let rows: Vec<RowSource<'_>> = entries
        .iter()
        .enumerate()
        .map(|(idx, e)| RowSource {
            record_id: ids[idx],
            css_classes: "time-entry",
            group: None,
            inline: vec![
                CellSource {
                    column: &cols[0],
                    css_classes: "",
                    data: CellData::PrimaryLink {
                        label: &e.spent_on,
                        href: &hrefs[idx],
                    },
                },
                CellSource {
                    column: &cols[1],
                    css_classes: "num",
                    data: CellData::Hours {
                        hours: &hours_strs[idx],
                        href: &hrefs[idx],
                    },
                },
                CellSource {
                    column: &cols[2],
                    css_classes: "",
                    data: CellData::Plain {
                        value: comments_strs[idx],
                    },
                },
            ],
            block: Vec::new(),
        })
        .collect();
    let body = render_list(
        "Time entries",
        0x0103,
        "billable_work_entry",
        &cols,
        &[],
        &rows,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc("Time entries", &body)))
}

/// `GET /time_entries/:id` — render a single time-entry's detail.
pub async fn detail(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Html<String>, HandlerError> {
    let rid = RecordId::new("time_entry", id_str.as_str());
    let entry: TimeEntryRow = state.store.find_time_entry(&rid).await?;
    let cols = detail_columns();
    let href = format!("/time_entries/{}", id_str);
    let hours_str = format!("{:.2}", entry.hours);
    let headline = format!(
        "<a href=\"{}\" class=\"primary-link\">{} — {}h</a>",
        href, &entry.spent_on, hours_str
    );
    let subtitle = entry.comments.as_deref().unwrap_or("");
    let cells = vec![
        CellSource {
            column: &cols[0],
            css_classes: "",
            data: CellData::Plain {
                value: &entry.spent_on,
            },
        },
        CellSource {
            column: &cols[1],
            css_classes: "num",
            data: CellData::Hours {
                hours: &hours_str,
                href: &href,
            },
        },
        CellSource {
            column: &cols[2],
            css_classes: "",
            data: CellData::Plain {
                value: entry.comments.as_deref().unwrap_or(""),
            },
        },
    ];
    let body = render_detail(
        0x0103,
        "billable_work_entry",
        record_id_to_u64(&rid),
        &headline,
        subtitle,
        &cols,
        &cells,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc(
        &format!("{} ({:.2}h)", &entry.spent_on, entry.hours),
        &body,
    )))
}

/// TimeEntry list-view columns: Date / Hours / Comments.
fn list_columns() -> [RenderColumn; 3] {
    [
        RenderColumn::new("spent_on", "Date", ColumnKind::PrimaryLink)
            .sortable()
            .frozen(),
        RenderColumn::new("hours", "Hours", ColumnKind::Hours).sortable(),
        RenderColumn::new("comments", "Comments", ColumnKind::Plain),
    ]
}

/// TimeEntry detail-view columns — same shape as the list today.
fn detail_columns() -> [RenderColumn; 3] {
    [
        RenderColumn::new("spent_on", "Date", ColumnKind::Plain),
        RenderColumn::new("hours", "Hours", ColumnKind::Hours),
        RenderColumn::new("comments", "Comments", ColumnKind::Plain),
    ]
}

/// Build the TimeEntry router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/time_entries", get(list))
        .route("/time_entries/:id", get(detail))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rm_store::{NewTimeEntry, Store};
    use tower::ServiceExt;

    async fn app_with_entries(seed: &[(f64, &str, Option<&str>)]) -> Router {
        let store = Store::open().await.expect("store boots");
        for (hours, spent_on, comments) in seed {
            store
                .create_time_entry(NewTimeEntry {
                    hours: *hours,
                    spent_on: spent_on.to_string(),
                    comments: comments.map(|s| s.to_string()),
                })
                .await
                .expect("seed insert");
        }
        router(AppState { store })
    }

    #[tokio::test]
    async fn list_renders_empty_state_when_no_entries() {
        let app = app_with_entries(&[]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/time_entries")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(
            s.contains("data-class-id=\"0x0103\""),
            "expected canonical class id 0x0103:\n{s}"
        );
        assert!(s.contains("No data."), "empty-state missing:\n{s}");
    }

    #[tokio::test]
    async fn list_renders_seeded_entries() {
        let app =
            app_with_entries(&[(1.5, "2026-06-20", Some("docs")), (2.0, "2026-06-21", None)]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/time_entries")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("2026-06-20"), "missing first date:\n{s}");
        assert!(s.contains("2026-06-21"), "missing second date:\n{s}");
        assert!(s.contains("docs"), "missing first comment:\n{s}");
        assert!(
            s.contains("href=\"/time_entries/"),
            "expected detail hrefs:\n{s}"
        );
    }

    #[tokio::test]
    async fn detail_renders_a_known_entry() {
        let store = Store::open().await.unwrap();
        let inserted = store
            .create_time_entry(NewTimeEntry {
                hours: 3.25,
                spent_on: "2026-06-21".to_string(),
                comments: Some("convergence migration".to_string()),
            })
            .await
            .unwrap();
        let rid = inserted.id.expect("inserted carries id");
        let key = rid.key.to_sql();
        let app = router(AppState { store });
        let res = app
            .oneshot(
                Request::builder()
                    .uri(format!("/time_entries/{key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("2026-06-21"), "missing date:\n{s}");
        assert!(s.contains("convergence migration"), "missing comment:\n{s}");
        assert!(
            s.contains("data-class-id=\"0x0103\""),
            "expected 0x0103:\n{s}"
        );
    }

    #[tokio::test]
    async fn detail_returns_404_for_unknown_entry() {
        let app = app_with_entries(&[]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/time_entries/missing_id")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
