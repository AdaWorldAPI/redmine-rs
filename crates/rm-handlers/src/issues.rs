//! **W1** — Issue (`project_work_item`) list + detail handlers.
//!
//! The hot-path resource: Redmine's `Issue`, OpenProject's
//! `WorkPackage`, both routing through the canonical
//! `class_ids::PROJECT_WORK_ITEM` (`0x0102`) arm.
//!
//! Routes (mounted by [`router`] at the workspace router):
//!
//! - `GET /issues` — list page with **filter / sort / paginate** chrome
//!   ([`ListQuery`]: `?q=`, `?sort=`, `?page=`, `?per_page=`).
//! - `GET /issues/:id` — detail page for a single issue.
//!
//! # Today's scope
//!
//! - Columns hardcoded; the `default_columns_for(&class)` factoring
//!   lands when W2 (Project) introduces a second caller.
//! - **D2** — filter by subject substring, sort by subject (asc/desc),
//!   paginate (25/page default, capped at 100). Status / priority /
//!   tracker / assignee filters wait until those FKs land on the
//!   `IssueRow` (W2/W3 taxonomy, W4 actor).
//! - No edit form (D1 — Plan §4). The create form will live in a
//!   sibling module.
//! - URL `:id` is the SurrealDB record-key segment (string ulid).
//!   The render-kit's u64 `record_id` parameter is filled via
//!   [`common::record_id_to_u64`] (stable hash); proper integer ids
//!   land when a Redmine-shaped `iid` column is added to the row.
//!
//! The filter / sort / pagination math runs in-memory on the result of
//! [`Store::list_issues`] today — a deliberate first step. Pushing the
//! predicate + `LIMIT/OFFSET` into the SurrealDB query is a follow-on
//! optimisation that doesn't change the handler's public shape; the
//! [`ListQuery`] extractor stays the same.
//!
//! [`Store::list_issues`]: rm_store::Store::list_issues

use axum::extract::{Path, Query, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use ogar_render_askama::{
    render_detail, render_list, CellData, CellSource, ColumnKind, RenderColumn, RowSource,
};
use rm_store::IssueRow;
use surrealdb_types::{RecordId, ToSql};

use crate::common::{html_escape, record_id_to_u64, wrap_in_doc, AppState, HandlerError};
use crate::list_chrome::{ListQuery, SortDir};

/// The list URL — used by the chrome to build self-referential links
/// (filter form action, sort headers, pagination Prev/Next).
const LIST_PATH: &str = "/issues";

/// The set of column names the `?sort=<col>` parameter accepts on the
/// issue list. Unknown columns silently fall back to insertion order so
/// a hostile URL never errors out the page.
const SORTABLE_COLUMNS: &[&str] = &["id", "subject"];

/// `GET /issues` — render the issue list with D2 filter / sort / page.
pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Html<String>, HandlerError> {
    let issues = state.store.list_issues().await?;
    let view = apply_filter_sort(&issues, &q);
    let total = view.len();
    let (start, end) = q.page_window(total);
    let page_view: &[&IssueRow] = &view[start..end];

    let cols = list_columns();
    // Row construction is allocation-light per the W1 doc; each row's
    // strings are borrowed from the IssueRow.
    let hrefs: Vec<String> = page_view
        .iter()
        .map(|i| {
            i.id.as_ref()
                .map(|rid| format!("/issues/{}", rid.key.to_sql()))
                .unwrap_or_default()
        })
        .collect();
    let ids: Vec<u64> = page_view
        .iter()
        .map(|i| i.id.as_ref().map(record_id_to_u64).unwrap_or(0))
        .collect();
    let rows: Vec<RowSource<'_>> = page_view
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
    let table = render_list("Issues", 0x0102, "project_work_item", &cols, &[], &rows)
        .map_err(|e| HandlerError::Render(e.to_string()))?;

    // Chrome composes around the table: filter bar above (with search +
    // clear), clickable sort headers (column-by-column links over the
    // existing table rows — emitted as a small nav block above the
    // table), and pagination strip below.
    let filter_bar = q.render_filter_bar(LIST_PATH, "Filter by subject");
    let sort_nav = render_sort_nav(&q);
    let pagination = q.render_pagination(LIST_PATH, total);
    let body = format!("{filter_bar}\n{sort_nav}\n{table}\n{pagination}");
    Ok(Html(wrap_in_doc("Issues", &body)))
}

/// Filter (subject substring, case-insensitive) + sort the list of
/// issues per the query. Returns a view of references to the originals
/// so the page-window slice never moves bytes.
fn apply_filter_sort<'a>(issues: &'a [IssueRow], q: &ListQuery) -> Vec<&'a IssueRow> {
    let needle = q.search_needle();
    let mut view: Vec<&IssueRow> = if needle.is_empty() {
        issues.iter().collect()
    } else {
        issues
            .iter()
            .filter(|i| i.subject.to_lowercase().contains(&needle))
            .collect()
    };
    // Only sort on columns we recognise — guards against a hostile
    // `?sort=<garbage>` triggering a partial-order on a column the row
    // doesn't carry today. `.filter()` collapses the column-allowlist
    // check into the pattern so clippy's `collapsible_if` doesn't fire
    // (workspace is edition 2021 — let-chains aren't available here).
    if let Some((col, dir)) = q.sort().filter(|(c, _)| SORTABLE_COLUMNS.contains(c)) {
        match col {
            "subject" => view.sort_by(|a, b| a.subject.cmp(&b.subject)),
            "id" => view.sort_by_key(|i| {
                i.id.as_ref()
                    .map(|rid| rid.key.to_sql())
                    .unwrap_or_default()
            }),
            _ => {} // SORTABLE_COLUMNS allow-list above
        }
        if dir == SortDir::Desc {
            view.reverse();
        }
    }
    view
}

/// Render the clickable column-header strip. Sortable columns become
/// links that emit `?sort=<col>:<dir>` URLs (preserving filter + page).
/// Active column gets an arrow indicator (↑ asc, ↓ desc). Kept terse —
/// the canonical render-kit list view already shows the column captions
/// in the table head; this strip is the *sort affordance* sitting above
/// the table, where Redmine users expect to click for re-ordering.
fn render_sort_nav(q: &ListQuery) -> String {
    use std::fmt::Write as _;
    let active = q.sort();
    let mut out = String::with_capacity(160);
    out.push_str(r#"<nav class="list-sort" aria-label="Sort">"#);
    out.push_str("Sort: ");
    for col in SORTABLE_COLUMNS {
        let href = q.sort_href(LIST_PATH, col);
        let indicator = match active {
            Some((c, SortDir::Asc)) if c == *col => " ↑",
            Some((c, SortDir::Desc)) if c == *col => " ↓",
            _ => "",
        };
        let _ = write!(
            &mut out,
            r#"<a class="sort-header" href="{}">{}{}</a>"#,
            href,
            html_escape(col),
            indicator
        );
    }
    out.push_str("</nav>");
    out
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
    // Subject is user-controlled; the kit treats `headline_html` as
    // already-rendered HTML, so we escape before composing the link
    // (codex P1 on PR #10).
    let headline = format!(
        "<a href=\"{}\" class=\"primary-link\">{}</a>",
        html_escape(&href),
        html_escape(&issue.subject)
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
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

    async fn body_of(app: Router, uri: &str) -> (StatusCode, String) {
        let res = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let s = String::from_utf8(bytes.to_vec()).unwrap();
        (status, s)
    }

    #[tokio::test]
    async fn list_renders_empty_state_when_no_issues() {
        let app = app_with_issues(&[]).await;
        let (status, s) = body_of(app, "/issues").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            s.contains("data-class-id=\"0x0102\""),
            "expected canonical class id in:\n{s}"
        );
        assert!(s.contains("No data."), "expected empty-state in:\n{s}");
        // Empty result → no pagination strip (the chrome's contract).
        assert!(
            !s.contains("Page 1 of"),
            "no pagination on empty list:\n{s}"
        );
        // But the filter bar always renders (so the user can type to add data).
        assert!(s.contains("list-filter"), "filter bar always renders:\n{s}");
    }

    #[tokio::test]
    async fn list_renders_seeded_issues() {
        let app =
            app_with_issues(&[("Fix the foo", Some("a description")), ("Bar broken", None)]).await;
        let (status, s) = body_of(app, "/issues").await;
        assert_eq!(status, StatusCode::OK);
        assert!(s.contains("Fix the foo"), "expected first subject in:\n{s}");
        assert!(s.contains("Bar broken"), "expected second subject in:\n{s}");
        assert!(
            s.contains("href=\"/issues/"),
            "expected detail hrefs in:\n{s}"
        );
        // Pagination strip shows the right total.
        assert!(s.contains("Page 1 of 1 (2)"), "expected pagination:\n{s}");
    }

    // ── D2: filter / sort / paginate ────────────────────────────────

    #[tokio::test]
    async fn list_filters_by_subject_substring_case_insensitive() {
        let app = app_with_issues(&[
            ("Fix the foo", None),
            ("Bar broken", None),
            ("Another FOO bug", None),
        ])
        .await;
        let (status, s) = body_of(app, "/issues?q=foo").await;
        assert_eq!(status, StatusCode::OK);
        assert!(s.contains("Fix the foo"), "kept matching first row:\n{s}");
        assert!(
            s.contains("Another FOO bug"),
            "case-insensitive match must keep this row:\n{s}"
        );
        assert!(
            !s.contains("Bar broken"),
            "non-match must be filtered:\n{s}"
        );
        // The filter bar should echo the query and offer a clear link.
        assert!(s.contains(r#"value="foo""#), "filter bar echoes q:\n{s}");
        assert!(s.contains("Clear"), "clear link when filter active:\n{s}");
        assert!(
            s.contains("(2)"),
            "pagination reflects filtered total:\n{s}"
        );
    }

    #[tokio::test]
    async fn list_sort_by_subject_asc_and_desc_changes_row_order() {
        let app = app_with_issues(&[("Charlie", None), ("Alpha", None), ("Bravo", None)]).await;
        let (_, asc) = body_of(app.clone(), "/issues?sort=subject:asc").await;
        let (_, desc) = body_of(app.clone(), "/issues?sort=subject:desc").await;
        let pos = |s: &str, needle: &str| s.find(needle).unwrap_or(usize::MAX);
        // Asc → Alpha, Bravo, Charlie
        assert!(
            pos(&asc, "Alpha") < pos(&asc, "Bravo") && pos(&asc, "Bravo") < pos(&asc, "Charlie"),
            "asc order broken:\n{asc}"
        );
        // Desc → Charlie, Bravo, Alpha
        assert!(
            pos(&desc, "Charlie") < pos(&desc, "Bravo")
                && pos(&desc, "Bravo") < pos(&desc, "Alpha"),
            "desc order broken:\n{desc}"
        );
        // Sort nav shows the indicator on the active direction.
        assert!(asc.contains("subject ↑"), "asc indicator missing:\n{asc}");
        assert!(
            desc.contains("subject ↓"),
            "desc indicator missing:\n{desc}"
        );
    }

    #[tokio::test]
    async fn list_paginates_with_per_page_cap_and_page_window() {
        // 5 issues, per_page=2 → 3 pages.
        let app = app_with_issues(&[
            ("alpha", None),
            ("bravo", None),
            ("charlie", None),
            ("delta", None),
            ("echo", None),
        ])
        .await;
        let (_, p1) = body_of(app.clone(), "/issues?per_page=2&sort=subject:asc").await;
        let (_, p2) = body_of(app.clone(), "/issues?per_page=2&page=2&sort=subject:asc").await;
        let (_, p9) = body_of(app.clone(), "/issues?per_page=2&page=99&sort=subject:asc").await;

        // Page 1 → alpha, bravo (sorted asc); page 2 → charlie, delta;
        // page 99 → no rows but no panic (empty body, pagination still
        // shows position clamped to last page).
        assert!(p1.contains("alpha"), "{p1}");
        assert!(p1.contains("bravo"), "{p1}");
        assert!(!p1.contains("charlie"), "page-1 must not bleed: {p1}");
        assert!(p2.contains("charlie") && p2.contains("delta"), "{p2}");
        assert!(!p2.contains("alpha"), "page-2 must not include alpha: {p2}");
        assert!(p2.contains("Page 2 of 3 (5)"), "pagination wrong:\n{p2}");
        // page 99 → page slice is empty; the kit's empty-state shows.
        assert!(p9.contains("No data."), "out-of-range page → empty:\n{p9}");
    }

    #[tokio::test]
    async fn list_per_page_clamps_hostile_values_to_redmine_band() {
        let app = app_with_issues(&[("a", None), ("b", None), ("c", None), ("d", None)]).await;
        // per_page=1000 → caps to 100; all 4 fit on one page.
        let (_, all) = body_of(app, "/issues?per_page=1000").await;
        assert!(all.contains("Page 1 of 1 (4)"), "expected 1 page:\n{all}");
        // No "Prev" / "Next" links on the single page.
        assert!(!all.contains("« Prev") && !all.contains("Next »"));
    }

    #[tokio::test]
    async fn list_ignores_unknown_sort_columns_silently() {
        // A hostile URL asking to sort on a column that doesn't exist on
        // IssueRow today (e.g. `priority`) must NOT error the request and
        // must NOT crash — it falls back to insertion order.
        let app = app_with_issues(&[("Aaa", None), ("Zzz", None)]).await;
        let (status, s) = body_of(app, "/issues?sort=priority:desc").await;
        assert_eq!(status, StatusCode::OK);
        // Both rows still rendered; the page didn't 4xx.
        assert!(s.contains("Aaa") && s.contains("Zzz"), "{s}");
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
        let (status, s) = body_of(app, &format!("/issues/{key}")).await;
        assert_eq!(status, StatusCode::OK);
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

    #[tokio::test]
    async fn detail_escapes_xss_in_subject_for_title_and_headline() {
        // Codex P1 on PR #10. Two paths both need escaping:
        // 1) the <title> via `wrap_in_doc(title=...)` — fixed in `common`
        // 2) the headline anchor passed as `headline_html` to render_detail —
        //    the kit treats that arg as already-rendered HTML and `|safe`s
        //    it through, so this handler must escape before composing.
        let store = Store::open().await.unwrap();
        let inserted = store
            .create_issue(NewIssue {
                subject: "</title><script>alert(1)</script>".to_string(),
                description: None,
            })
            .await
            .unwrap();
        let rid = inserted.id.unwrap();
        let key = rid.key.to_sql();
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
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(
            !s.contains("</title><script>"),
            "raw subject survived into title — XSS:\n{s}"
        );
        assert!(
            !s.contains("<script>alert(1)"),
            "raw script tag survived into headline — XSS:\n{s}"
        );
        // The escaped form should appear (in <title> AND in the
        // headline anchor text).
        assert!(
            s.contains("&lt;/title&gt;&lt;script&gt;alert(1)"),
            "expected escaped form somewhere:\n{s}"
        );
    }

    #[tokio::test]
    async fn list_filter_input_value_is_xss_safe() {
        // The filter bar echoes ?q= into a <input value="..."> — must
        // HTML-escape so a hostile `?q="><script>alert(1)</script>`
        // never breaks out of the attribute.
        let app = app_with_issues(&[("foo", None)]).await;
        let (_, s) = body_of(
            app,
            r#"/issues?q=%22%3E%3Cscript%3Ealert(1)%3C%2Fscript%3E"#,
        )
        .await;
        assert!(
            !s.contains("\"><script>alert(1)</script>"),
            "raw payload survived into filter input — XSS:\n{s}"
        );
        assert!(
            s.contains("&quot;&gt;&lt;script&gt;alert(1)"),
            "expected escaped form in filter input:\n{s}"
        );
    }
}
