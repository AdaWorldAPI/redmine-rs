//! Home / overview page (`GET /`).
//!
//! Redmine's landing page is a jumping-off point: a heading plus links
//! into every resource. This is the rm-* equivalent — a single overview
//! that lists each seeded concept with its live row count and links to
//! that resource's list route. It's the first thing a demo visitor sees,
//! so it doubles as the site map until G1's master template lands a
//! persistent top nav.
//!
//! Unlike the W1..W8 resource modules this one is **cross-cutting**: it
//! reads a count from every store table. On the in-memory store that's a
//! cheap `list_*().len()`; when a SQL backend lands these become
//! `COUNT(*)` and the handler shape doesn't change.
//!
//! Per Plan §8 file ownership it still owns exactly one file + one route
//! (`/`), so it merges into `rm-server` the same way every W* track does.

use std::fmt::Write as _;

use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use axum::Router;

use crate::common::{html_escape, wrap_in_doc, AppState, HandlerError};

/// One row of the overview index: a human label, the list route, and the
/// live count of records behind it. Labels + hrefs are static (the route
/// table is fixed); only `count` is read from the store.
struct ResourceEntry {
    label: &'static str,
    href: &'static str,
    count: usize,
}

/// `GET /` — the overview / landing page.
///
/// Reads one count per seeded resource and renders the index. Any store
/// error short-circuits to a 500 via `?` — a dead store means nothing
/// renders anyway, so there's no partial-page to salvage.
pub async fn home(State(state): State<AppState>) -> Result<Html<String>, HandlerError> {
    let s = &state.store;
    // Counts in Redmine's sidebar grouping: work first, then taxonomy,
    // then people, then SCM / meta. Each `?` bubbles a store failure.
    let entries = [
        ResourceEntry {
            label: "Issues",
            href: "/issues",
            count: s.list_issues().await?.len(),
        },
        ResourceEntry {
            label: "Projects",
            href: "/projects",
            count: s.list_projects().await?.len(),
        },
        ResourceEntry {
            label: "Time entries",
            href: "/time_entries",
            count: s.list_time_entries().await?.len(),
        },
        ResourceEntry {
            label: "News",
            href: "/news",
            count: s.list_news().await?.len(),
        },
        ResourceEntry {
            label: "Wiki pages",
            href: "/wiki",
            count: s.list_wiki_pages().await?.len(),
        },
        ResourceEntry {
            label: "Users",
            href: "/users",
            count: s.list_users().await?.len(),
        },
        ResourceEntry {
            label: "Roles",
            href: "/roles",
            count: s.list_roles().await?.len(),
        },
        ResourceEntry {
            label: "Repositories",
            href: "/repositories",
            count: s.list_repositories().await?.len(),
        },
        ResourceEntry {
            label: "Issue statuses",
            href: "/issue_statuses",
            count: s.list_issue_statuses().await?.len(),
        },
        ResourceEntry {
            label: "Trackers",
            href: "/trackers",
            count: s.list_trackers().await?.len(),
        },
        ResourceEntry {
            label: "Priorities",
            href: "/enumerations/issue_priorities",
            count: s.list_issue_priorities().await?.len(),
        },
        ResourceEntry {
            label: "Custom queries",
            href: "/queries",
            count: s.list_queries().await?.len(),
        },
        ResourceEntry {
            label: "Relations",
            href: "/relations",
            count: s.list_relations().await?.len(),
        },
    ];
    let body = render_home(&entries);
    Ok(Html(wrap_in_doc("Home", &body)))
}

/// Pure render of the overview page from the resource counts. Split from
/// [`home`] so the markup is unit-testable without booting a store. All
/// labels + hrefs are HTML-escaped (uniform with the rest of the chrome,
/// even though today's values are static).
fn render_home(entries: &[ResourceEntry]) -> String {
    let mut out = String::with_capacity(512 + 96 * entries.len());
    out.push_str(r#"<main class="home">"#);
    out.push_str("<h1>Redmine RS</h1>");
    out.push_str(
        r#"<p class="tagline">A faithful Redmine port on the OGAR canonical-vocabulary render kit.</p>"#,
    );
    // Quick-action row mirrors Redmine's home "+ New issue" jump.
    out.push_str(r#"<nav class="contextual" aria-label="Quick actions">"#);
    out.push_str(r#"<a class="action" href="/issues/new">New issue</a>"#);
    out.push_str("</nav>");
    out.push_str(r#"<section class="overview"><h2>Overview</h2><ul class="resource-index">"#);
    for e in entries {
        let _ = write!(
            &mut out,
            r#"<li class="resource"><a href="{href}">{label}</a> <span class="count">{count}</span></li>"#,
            href = html_escape(e.href),
            label = html_escape(e.label),
            count = e.count,
        );
    }
    out.push_str("</ul></section></main>");
    out
}

/// Build the home router. `rm-server` merges this at `/` — it's the one
/// route that owns the document root, so it sorts first in the merge
/// block.
pub fn router(state: AppState) -> Router {
    Router::new().route("/", get(home)).with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rm_store::{NewIssue, Store};
    use tower::ServiceExt;

    #[test]
    fn render_home_lists_entries_with_counts_and_links() {
        let entries = [
            ResourceEntry {
                label: "Issues",
                href: "/issues",
                count: 27,
            },
            ResourceEntry {
                label: "Projects",
                href: "/projects",
                count: 3,
            },
        ];
        let html = render_home(&entries);
        assert!(html.contains(r#"href="/issues""#), "{html}");
        assert!(html.contains(">Issues</a>"), "{html}");
        assert!(
            html.contains(r#"<span class="count">27</span>"#),
            "issue count missing:\n{html}"
        );
        assert!(html.contains(r#"href="/projects""#), "{html}");
        assert!(
            html.contains(r#"<span class="count">3</span>"#),
            "project count missing:\n{html}"
        );
        // The New issue quick-action is always present (it doesn't depend
        // on the counts).
        assert!(
            html.contains(r#"href="/issues/new""#),
            "new-issue quick action missing:\n{html}"
        );
    }

    #[tokio::test]
    async fn home_route_renders_overview_with_live_counts() {
        let store = Store::open().await.expect("store boots");
        // Seed three issues so the issues row count is observable end-to-end.
        for subject in ["one", "two", "three"] {
            store
                .create_issue(NewIssue {
                    subject: subject.to_string(),
                    description: None,
                })
                .await
                .expect("seed insert");
        }
        let app = router(AppState { store });
        let res = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let s = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(s.contains("Redmine RS"), "title heading missing:\n{s}");
        assert!(s.contains(r#"href="/issues""#), "issues link missing:\n{s}");
        // 3 seeded issues → the issues row shows count 3.
        assert!(
            s.contains(r#"<span class="count">3</span>"#),
            "expected issue count 3:\n{s}"
        );
        // Cross-resource: the projects link renders even with 0 projects,
        // proving the overview spans every table, not just the seeded one.
        assert!(
            s.contains(r#"href="/projects""#),
            "projects link missing:\n{s}"
        );
    }
}
