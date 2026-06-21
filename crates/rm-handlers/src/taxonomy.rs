//! **W5** — Taxonomy admin pages: IssueStatus, Tracker, IssuePriority.
//!
//! Three near-identical lookup resources sharing the
//! `(name, position, is_<flag>)` shape. Each routes through a
//! distinct OGAR class_id so the rendered HTML carries the right
//! `data-class-id` attribute for downstream JS hooks.
//!
//! Routes (mounted by [`router`]):
//!
//! - `GET /issue_statuses`  + `/issue_statuses/:name`  (0x0105)
//! - `GET /trackers`        + `/trackers/:name`        (0x0106)
//! - `GET /enumerations/issue_priorities` + `.../issue_priorities/:name` (0x0107)
//!
//! The IssuePriority URL follows Redmine's convention
//! (`/enumerations/issue_priorities` — Enumerations group lookup
//! tables of variable schemas, and Redmine ships several including
//! TimeEntryActivity + DocumentCategory that we don't model yet).

use axum::extract::{Path, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use ogar_render_askama::{
    render_detail, render_list, CellData, CellSource, ColumnKind, RenderColumn, RowSource,
};
use rm_store::{IssuePriorityRow, IssueStatusRow, TrackerRow};

use crate::common::{
    encode_path_segment, html_escape, identifier_to_u64, wrap_in_doc, AppState, HandlerError,
};

// ── IssueStatus handlers (0x0105) ───────────────────────────────────

/// `GET /issue_statuses` — render the issue-status list.
pub async fn issue_status_list(
    State(state): State<AppState>,
) -> Result<Html<String>, HandlerError> {
    let rows = state.store.list_issue_statuses().await?;
    let cols = list_columns_with_flag("Closed?");
    let hrefs: Vec<String> = rows
        .iter()
        .map(|r| format!("/issue_statuses/{}", encode_path_segment(&r.name)))
        .collect();
    let positions: Vec<String> = rows.iter().map(|r| r.position.to_string()).collect();
    let flags: Vec<&'static str> = rows
        .iter()
        .map(|r| if r.is_closed { "yes" } else { "no" })
        .collect();
    let ids: Vec<u64> = rows.iter().map(|r| identifier_to_u64(&r.name)).collect();
    let row_sources: Vec<RowSource<'_>> = rows
        .iter()
        .enumerate()
        .map(|(idx, r)| {
            build_lookup_row(
                idx,
                ids[idx],
                &r.name,
                &hrefs[idx],
                &positions[idx],
                flags[idx],
                &cols,
                "issue-status",
            )
        })
        .collect();
    let body = render_list(
        "Issue statuses",
        0x0105,
        "project_status",
        &cols,
        &[],
        &row_sources,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc("Issue statuses", &body)))
}

/// `GET /issue_statuses/:name` — render an issue-status detail page.
pub async fn issue_status_detail(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Html<String>, HandlerError> {
    let row: IssueStatusRow = state.store.find_issue_status_by_name(&name).await?;
    let href = format!("/issue_statuses/{}", encode_path_segment(&row.name));
    let position_str = row.position.to_string();
    let flag_str = if row.is_closed { "yes" } else { "no" };
    detail_render(
        0x0105,
        "project_status",
        &row.name,
        &href,
        &position_str,
        flag_str,
        "Closed?",
    )
}

// ── Tracker handlers (0x0106) ───────────────────────────────────────

/// `GET /trackers` — render the tracker list.
pub async fn tracker_list(State(state): State<AppState>) -> Result<Html<String>, HandlerError> {
    let rows = state.store.list_trackers().await?;
    let cols = list_columns_with_flag("Default?");
    let hrefs: Vec<String> = rows
        .iter()
        .map(|r| format!("/trackers/{}", encode_path_segment(&r.name)))
        .collect();
    let positions: Vec<String> = rows.iter().map(|r| r.position.to_string()).collect();
    let flags: Vec<&'static str> = rows
        .iter()
        .map(|r| if r.is_default { "yes" } else { "no" })
        .collect();
    let ids: Vec<u64> = rows.iter().map(|r| identifier_to_u64(&r.name)).collect();
    let row_sources: Vec<RowSource<'_>> = rows
        .iter()
        .enumerate()
        .map(|(idx, r)| {
            build_lookup_row(
                idx,
                ids[idx],
                &r.name,
                &hrefs[idx],
                &positions[idx],
                flags[idx],
                &cols,
                "tracker",
            )
        })
        .collect();
    let body = render_list("Trackers", 0x0106, "project_type", &cols, &[], &row_sources)
        .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc("Trackers", &body)))
}

/// `GET /trackers/:name` — render a tracker detail page.
pub async fn tracker_detail(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Html<String>, HandlerError> {
    let row: TrackerRow = state.store.find_tracker_by_name(&name).await?;
    let href = format!("/trackers/{}", encode_path_segment(&row.name));
    let position_str = row.position.to_string();
    let flag_str = if row.is_default { "yes" } else { "no" };
    detail_render(
        0x0106,
        "project_type",
        &row.name,
        &href,
        &position_str,
        flag_str,
        "Default?",
    )
}

// ── IssuePriority handlers (0x0107) ─────────────────────────────────

/// `GET /enumerations/issue_priorities` — render the issue-priority list.
pub async fn issue_priority_list(
    State(state): State<AppState>,
) -> Result<Html<String>, HandlerError> {
    let rows = state.store.list_issue_priorities().await?;
    let cols = list_columns_with_flag("Default?");
    let hrefs: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "/enumerations/issue_priorities/{}",
                encode_path_segment(&r.name)
            )
        })
        .collect();
    let positions: Vec<String> = rows.iter().map(|r| r.position.to_string()).collect();
    let flags: Vec<&'static str> = rows
        .iter()
        .map(|r| if r.is_default { "yes" } else { "no" })
        .collect();
    let ids: Vec<u64> = rows.iter().map(|r| identifier_to_u64(&r.name)).collect();
    let row_sources: Vec<RowSource<'_>> = rows
        .iter()
        .enumerate()
        .map(|(idx, r)| {
            build_lookup_row(
                idx,
                ids[idx],
                &r.name,
                &hrefs[idx],
                &positions[idx],
                flags[idx],
                &cols,
                "issue-priority",
            )
        })
        .collect();
    let body = render_list(
        "Issue priorities",
        0x0107,
        "priority",
        &cols,
        &[],
        &row_sources,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc("Issue priorities", &body)))
}

/// `GET /enumerations/issue_priorities/:name` — render a priority detail page.
pub async fn issue_priority_detail(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Html<String>, HandlerError> {
    let row: IssuePriorityRow = state.store.find_issue_priority_by_name(&name).await?;
    let href = format!(
        "/enumerations/issue_priorities/{}",
        encode_path_segment(&row.name)
    );
    let position_str = row.position.to_string();
    let flag_str = if row.is_default { "yes" } else { "no" };
    detail_render(
        0x0107,
        "priority",
        &row.name,
        &href,
        &position_str,
        flag_str,
        "Default?",
    )
}

// ── Shared row + render helpers ─────────────────────────────────────

fn list_columns_with_flag(flag_caption: &'static str) -> [RenderColumn; 3] {
    [
        RenderColumn::new("name", "Name", ColumnKind::PrimaryLink)
            .sortable()
            .frozen(),
        RenderColumn::new("position", "Position", ColumnKind::Plain).sortable(),
        RenderColumn::new("flag", flag_caption, ColumnKind::Plain),
    ]
}

fn detail_columns(flag_caption: &'static str) -> [RenderColumn; 3] {
    [
        RenderColumn::new("name", "Name", ColumnKind::PrimaryLink),
        RenderColumn::new("position", "Position", ColumnKind::Plain),
        RenderColumn::new("flag", flag_caption, ColumnKind::Plain),
    ]
}

#[allow(clippy::too_many_arguments)]
fn build_lookup_row<'a>(
    _idx: usize,
    record_id: u64,
    name: &'a str,
    href: &'a str,
    position: &'a str,
    flag: &'a str,
    cols: &'a [RenderColumn; 3],
    css_class: &'static str,
) -> RowSource<'a> {
    RowSource {
        record_id,
        css_classes: css_class,
        group: None,
        inline: vec![
            CellSource {
                column: &cols[0],
                css_classes: "",
                data: CellData::PrimaryLink { label: name, href },
            },
            CellSource {
                column: &cols[1],
                css_classes: "num",
                data: CellData::Plain { value: position },
            },
            CellSource {
                column: &cols[2],
                css_classes: "flag",
                data: CellData::Plain { value: flag },
            },
        ],
        block: Vec::new(),
    }
}

fn detail_render(
    class_id: u16,
    concept: &'static str,
    name: &str,
    href: &str,
    position_str: &str,
    flag_str: &str,
    flag_caption: &'static str,
) -> Result<Html<String>, HandlerError> {
    let cols = detail_columns(flag_caption);
    let headline = format!(
        "<a href=\"{}\" class=\"primary-link\">{}</a>",
        html_escape(href),
        html_escape(name)
    );
    let cells = vec![
        CellSource {
            column: &cols[0],
            css_classes: "",
            data: CellData::PrimaryLink { label: name, href },
        },
        CellSource {
            column: &cols[1],
            css_classes: "num",
            data: CellData::Plain {
                value: position_str,
            },
        },
        CellSource {
            column: &cols[2],
            css_classes: "flag",
            data: CellData::Plain { value: flag_str },
        },
    ];
    let body = render_detail(
        class_id,
        concept,
        identifier_to_u64(name),
        &headline,
        position_str,
        &cols,
        &cells,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc(name, &body)))
}

// ── Router ──────────────────────────────────────────────────────────

/// Build the Taxonomy router (mounts all three resources). One
/// merge call in rm-server brings the lot.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/issue_statuses", get(issue_status_list))
        .route("/issue_statuses/:name", get(issue_status_detail))
        .route("/trackers", get(tracker_list))
        .route("/trackers/:name", get(tracker_detail))
        .route("/enumerations/issue_priorities", get(issue_priority_list))
        .route(
            "/enumerations/issue_priorities/:name",
            get(issue_priority_detail),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rm_store::{NewIssuePriority, NewIssueStatus, NewTracker, Store};
    use tower::ServiceExt;

    async fn store_with_seed() -> Store {
        let store = Store::open().await.unwrap();
        store
            .create_issue_status(NewIssueStatus {
                name: "New".to_string(),
                position: 1,
                is_closed: false,
            })
            .await
            .unwrap();
        store
            .create_issue_status(NewIssueStatus {
                name: "Closed".to_string(),
                position: 99,
                is_closed: true,
            })
            .await
            .unwrap();
        store
            .create_tracker(NewTracker {
                name: "Bug".to_string(),
                position: 1,
                is_default: true,
            })
            .await
            .unwrap();
        store
            .create_issue_priority(NewIssuePriority {
                name: "Normal".to_string(),
                position: 2,
                is_default: true,
            })
            .await
            .unwrap();
        store
    }

    async fn issue_status_list_body(store: Store) -> String {
        let app = router(AppState { store });
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/issue_statuses")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    }

    #[tokio::test]
    async fn issue_status_list_renders_canonical_class_id() {
        let store = store_with_seed().await;
        let s = issue_status_list_body(store).await;
        assert!(s.contains("data-class-id=\"0x0105\""), "{s}");
        assert!(s.contains("New"));
        assert!(s.contains("Closed"));
    }

    #[tokio::test]
    async fn tracker_list_renders_default_flag() {
        let app = router(AppState {
            store: store_with_seed().await,
        });
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/trackers")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("data-class-id=\"0x0106\""));
        assert!(s.contains("Bug"));
        assert!(s.contains("href=\"/trackers/Bug\""));
        assert!(s.contains("yes"), "expected `yes` flag in:\n{s}");
    }

    #[tokio::test]
    async fn issue_priority_list_uses_enumerations_url() {
        let app = router(AppState {
            store: store_with_seed().await,
        });
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/enumerations/issue_priorities")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("data-class-id=\"0x0107\""));
        assert!(s.contains("Normal"));
        assert!(
            s.contains("href=\"/enumerations/issue_priorities/Normal\""),
            "expected Redmine-shape Enumerations URL:\n{s}"
        );
    }

    #[tokio::test]
    async fn detail_pages_render_for_each_resource() {
        let app = router(AppState {
            store: store_with_seed().await,
        });
        for (path, class_id) in [
            ("/issue_statuses/Closed", "0x0105"),
            ("/trackers/Bug", "0x0106"),
            ("/enumerations/issue_priorities/Normal", "0x0107"),
        ] {
            let res = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK, "path: {path}");
            let body = res.into_body().collect().await.unwrap().to_bytes();
            let s = std::str::from_utf8(&body).unwrap();
            assert!(
                s.contains(&format!("data-class-id=\"{class_id}\"")),
                "path {path} should carry {class_id}:\n{s}"
            );
        }
    }

    #[tokio::test]
    async fn detail_404_for_unknown_name_across_resources() {
        let app = router(AppState {
            store: Store::open().await.unwrap(),
        });
        for path in [
            "/issue_statuses/Nope",
            "/trackers/Nope",
            "/enumerations/issue_priorities/Nope",
        ] {
            let res = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }
}
