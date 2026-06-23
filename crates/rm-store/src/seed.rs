//! **POC autohydrate** — idempotent demo-data seeding.
//!
//! Boot of an in-memory SurrealDB instance starts EMPTY: every list page
//! shows "No data." and the filter / sort / paginate chrome has nothing to
//! filter. That's correct for production but unusable as a POC demo. This
//! module seeds a small, Redmine-shaped corpus the first time the store
//! comes up empty so a fresh `cargo run -p rm-server` lands on populated
//! list pages.
//!
//! # Idempotent by construction
//!
//! [`hydrate_demo_data`] checks for any existing issues; if `list_issues`
//! returns a non-empty `Vec` the seed is a no-op. So:
//!
//! - **First boot** (empty in-memory store) → seed runs, ~50 rows land.
//! - **Subsequent boots in the same process** (after a `create_issue` from
//!   the UI) → seed is a no-op; the user's data is never duplicated.
//! - **File-backed store with existing data** (when the rocksdb / surrealkv
//!   feature replaces `kv-mem` later) → seed is a no-op; production data
//!   stays untouched.
//!
//! # Operator override
//!
//! Set `RM_SEED=off` (or `0` / `false` / `no`) in the environment to skip
//! the seed regardless of store contents — useful in prod where the demo
//! corpus would be noise. Read once at boot via [`seed_enabled`]; the
//! pure [`seed_enabled_for`] is the testable core.
//!
//! Boot wrapper: call [`hydrate_demo_data_on_boot`] from the server's
//! startup path — it consults `RM_SEED` and short-circuits without
//! touching the store when disabled.
//!
//! # The demo corpus
//!
//! Sized so D2's filter / sort / paginate chrome has real work to do —
//! enough issues to exceed `per_page=25`, varied subject vocabulary so
//! substring filter exercises match + no-match paths, and a spread of
//! taxonomy rows so the eventual W2/W3/W4 facet filters have material
//! when their FKs land.

use crate::{
    NewIssue, NewIssuePriority, NewIssueStatus, NewNews, NewProject, NewQuery, NewRelation,
    NewRepository, NewRole, NewTimeEntry, NewTracker, NewUser, NewWikiPage, Store, StoreError,
};

/// Pure: given the value of `RM_SEED` (if any), should the seed run?
///
/// `None` (var absent) → `true` (default-on for POC ergonomics).
/// `Some("off")` / `"0"` / `"false"` / `"no"` (case-insensitive,
/// trimmed) → `false`. Anything else (`"on"`, `"1"`, empty, …) →
/// `true`. Testable without touching process env (the crate is
/// `#![forbid(unsafe_code)]`; `std::env::set_var` is unsafe in recent
/// Rust, so tests target this pure form).
#[must_use]
pub fn seed_enabled_for(env: Option<&str>) -> bool {
    match env {
        Some(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "off" | "0" | "false" | "no"
        ),
        None => true,
    }
}

/// Boot-time wrapper: read `RM_SEED` and delegate to [`seed_enabled_for`].
/// Used by [`hydrate_demo_data_on_boot`]; exposed `pub` for callers that
/// want to log the decision before calling the seed.
#[must_use]
pub fn seed_enabled() -> bool {
    seed_enabled_for(std::env::var("RM_SEED").ok().as_deref())
}

/// Seed the store with the demo corpus iff it's empty (idempotent).
/// Always seeds when called — does **not** consult `RM_SEED`. Returns
/// the total row count inserted (0 when the store was already populated).
///
/// For boot-time use that also honours the operator override, call
/// [`hydrate_demo_data_on_boot`] instead.
///
/// # Errors
///
/// Any [`StoreError`] surfaced by the underlying `create_*` calls. The
/// "is it empty?" check is infallible (an empty Vec is not an error).
pub async fn hydrate_demo_data(store: &Store) -> Result<usize, StoreError> {
    let already = store.list_issues().await?.len();
    if already > 0 {
        tracing::debug!(rows = already, "store already has issues — skipping seed");
        return Ok(0);
    }
    tracing::info!("seeding demo data (store empty)…");
    let mut total = 0usize;

    // ── Taxonomy: statuses + trackers + priorities ──────────────────
    for (i, (name, is_closed)) in [
        ("New", false),
        ("In Progress", false),
        ("Resolved", false),
        ("Closed", true),
    ]
    .iter()
    .enumerate()
    {
        store
            .create_issue_status(NewIssueStatus {
                name: (*name).to_string(),
                position: i as i64 + 1,
                is_closed: *is_closed,
            })
            .await?;
        total += 1;
    }
    for (i, (name, is_default)) in [("Bug", true), ("Feature", false), ("Support", false)]
        .iter()
        .enumerate()
    {
        store
            .create_tracker(NewTracker {
                name: (*name).to_string(),
                position: i as i64 + 1,
                is_default: *is_default,
            })
            .await?;
        total += 1;
    }
    for (i, (name, is_default)) in [
        ("Low", false),
        ("Normal", true),
        ("High", false),
        ("Urgent", false),
    ]
    .iter()
    .enumerate()
    {
        store
            .create_issue_priority(NewIssuePriority {
                name: (*name).to_string(),
                position: i as i64 + 1,
                is_default: *is_default,
            })
            .await?;
        total += 1;
    }

    // ── Projects ────────────────────────────────────────────────────
    for (name, identifier) in [
        ("Sample Project", "sample"),
        ("Demo App", "demo-app"),
        ("Documentation", "docs"),
    ] {
        store
            .create_project(NewProject {
                name: name.to_string(),
                identifier: identifier.to_string(),
            })
            .await?;
        total += 1;
    }

    // ── Users ───────────────────────────────────────────────────────
    for (login, display_name) in [
        ("admin", "Administrator"),
        ("alice", "Alice Engineer"),
        ("bob", "Bob Developer"),
        ("carol", "Carol Designer"),
    ] {
        store
            .create_user(NewUser {
                login: login.to_string(),
                display_name: display_name.to_string(),
            })
            .await?;
        total += 1;
    }

    // ── Roles ───────────────────────────────────────────────────────
    for (i, name) in ["Manager", "Developer", "Reporter"].iter().enumerate() {
        store
            .create_role(NewRole {
                name: (*name).to_string(),
                position: i as i64 + 1,
            })
            .await?;
        total += 1;
    }

    // ── Issues (the headline list — enough to fill >1 page) ─────────
    // Mixed subjects so D2's substring filter has match + no-match
    // partitions to exercise; spread alphabetically so sort by subject
    // produces a visibly-different page-1 result vs page-2.
    let issues: &[(&str, Option<&str>)] = &[
        (
            "Add login form to the home page",
            Some("First-pass auth — gated."),
        ),
        (
            "Bug: dashboard 500 on empty state",
            Some("Reproduces in dev."),
        ),
        ("Calendar widget overflows on mobile", None),
        ("Create export-to-CSV for issues", None),
        (
            "Database migration fails for sqlite",
            Some("FK constraint mismatch."),
        ),
        ("Email notifications go to spam", None),
        ("Fix typo on the readme", None),
        ("Gantt chart fails on long projects", None),
        (
            "Improve issue list pagination",
            Some("Per-page jumper would be nice."),
        ),
        (
            "Issue filter doesn't preserve sort",
            Some("Reported by alice."),
        ),
        ("LDAP login broken behind the proxy", None),
        ("Markdown preview button missing", None),
        ("New issue form needs validation", None),
        ("Optimize the query for /issues", None),
        ("Refactor the routes file", None),
        ("Search across wiki pages", Some("D6 — search-and-preview.")),
        ("Time entries don't sum correctly", None),
        ("Upgrade to Rust 2024 edition", None),
        ("Validate identifier format on project create", None),
        ("Wiki page history is missing", None),
        ("Zero-state empty page for /news", None),
        ("Add CSV import for time entries", None),
        ("Bug: forum thread sort by reply count", None),
        ("Change default tracker to Feature", None),
        (
            "Document the seed-data convention",
            Some("Meta — this row."),
        ),
        ("Enable per-project wiki", None),
        ("Fix unicode in user display name", None),
    ];
    for (subject, desc) in issues {
        store
            .create_issue(NewIssue {
                subject: (*subject).to_string(),
                description: desc.map(str::to_string),
            })
            .await?;
        total += 1;
    }

    // ── Time entries ────────────────────────────────────────────────
    // String date — see NewTimeEntry's doc; the SurrealValue conversion
    // to a proper Date type lands when D2's date-range filter needs it.
    for (hours, spent_on, comments) in [
        (4.0, "2026-06-20", Some("kickoff meeting")),
        (2.5, "2026-06-20", Some("investigation")),
        (1.0, "2026-06-21", None),
        (3.25, "2026-06-22", Some("D2 chrome polish")),
        (0.5, "2026-06-23", Some("review")),
    ] {
        store
            .create_time_entry(NewTimeEntry {
                hours,
                spent_on: spent_on.to_string(),
                comments: comments.map(str::to_string),
            })
            .await?;
        total += 1;
    }

    // ── News, wiki, repositories, queries, relations ────────────────
    for (title, summary, description) in [
        (
            "Welcome to the POC",
            "Seeded demo data is in.",
            "This is the autohydrated seed corpus that loads on first boot.",
        ),
        (
            "Sprint review notes",
            "Wrapping the D-track depth.",
            "Filter / sort / paginate landed; create form too.",
        ),
    ] {
        store
            .create_news(NewNews {
                title: title.to_string(),
                summary: summary.to_string(),
                description: description.to_string(),
            })
            .await?;
        total += 1;
    }

    for (title, body) in [
        (
            "Home",
            "# Welcome\nThis wiki page was seeded on first boot.",
        ),
        ("Conventions", "## Coding conventions\nSee CLAUDE.md."),
    ] {
        store
            .create_wiki_page(NewWikiPage {
                title: title.to_string(),
                body: body.to_string(),
            })
            .await?;
        total += 1;
    }

    for url in [
        "https://github.com/AdaWorldAPI/redmine-rs",
        "https://github.com/AdaWorldAPI/OGAR",
    ] {
        store
            .create_repository(NewRepository {
                url: url.to_string(),
                scm_type: "Git".to_string(),
            })
            .await?;
        total += 1;
    }

    for name in ["Open bugs", "My issues", "Recently updated"] {
        store
            .create_query(NewQuery {
                name: name.to_string(),
            })
            .await?;
        total += 1;
    }

    // One sample IssueRelation so the W8b list/detail has data.
    store
        .create_relation(NewRelation {
            relation_type: "precedes".to_string(),
            lag: 3,
        })
        .await?;
    total += 1;

    tracing::info!(rows = total, "demo data seeded");
    Ok(total)
}

/// Boot-time wrapper: honour `RM_SEED` and delegate to
/// [`hydrate_demo_data`]. Returns `Ok(0)` when seeding is disabled.
///
/// # Errors
///
/// Same as [`hydrate_demo_data`] when the seed actually runs; never
/// errors on the disabled path.
pub async fn hydrate_demo_data_on_boot(store: &Store) -> Result<usize, StoreError> {
    if !seed_enabled() {
        tracing::info!("RM_SEED disabled — skipping demo data hydration");
        return Ok(0);
    }
    hydrate_demo_data(store).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_enabled_for_default_on_when_var_absent() {
        assert!(seed_enabled_for(None));
    }

    #[test]
    fn seed_enabled_for_disables_on_disable_values() {
        for v in ["off", "OFF", "Off", "0", "false", "FALSE", "no", "No"] {
            assert!(
                !seed_enabled_for(Some(v)),
                "value {v:?} must disable seeding"
            );
        }
    }

    #[test]
    fn seed_enabled_for_keeps_on_for_non_disable_values() {
        for v in ["on", "1", "true", "yes", "", "anything"] {
            assert!(
                seed_enabled_for(Some(v)),
                "value {v:?} must keep default-on"
            );
        }
    }

    #[test]
    fn seed_enabled_for_trims_whitespace() {
        assert!(!seed_enabled_for(Some("  off  ")));
        assert!(!seed_enabled_for(Some("\t0\n")));
    }

    #[tokio::test]
    async fn fresh_store_gets_a_useful_corpus() {
        let store = Store::open().await.unwrap();
        let rows = hydrate_demo_data(&store).await.unwrap();
        // The exact number matters less than the shape — enough issues
        // to exceed one page (per_page=25 default), some of every other
        // resource. Tighten the bound if it changes intentionally.
        assert!(
            rows >= 50,
            "expected the seed to land a useful corpus, got {rows}"
        );
        let issues = store.list_issues().await.unwrap();
        assert!(
            issues.len() > 25,
            "expected enough issues to fill more than one page, got {}",
            issues.len()
        );
        // Shape probes — taxonomy + projects + users seeded.
        assert!(store.list_issue_statuses().await.unwrap().len() >= 4);
        assert!(store.list_trackers().await.unwrap().len() >= 3);
        assert!(store.list_issue_priorities().await.unwrap().len() >= 4);
        assert!(store.list_projects().await.unwrap().len() >= 3);
        assert!(store.list_users().await.unwrap().len() >= 4);
        assert!(store.list_roles().await.unwrap().len() >= 3);
        assert!(store.list_news().await.unwrap().len() >= 2);
        assert!(store.list_wiki_pages().await.unwrap().len() >= 2);
        assert!(store.list_time_entries().await.unwrap().len() >= 5);
        assert!(store.list_repositories().await.unwrap().len() >= 2);
        assert!(store.list_queries().await.unwrap().len() >= 3);
        assert!(!store.list_relations().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn second_call_is_idempotent_no_op() {
        let store = Store::open().await.unwrap();
        let first = hydrate_demo_data(&store).await.unwrap();
        assert!(first > 0, "first call must seed");
        let issues_after_first = store.list_issues().await.unwrap().len();
        let second = hydrate_demo_data(&store).await.unwrap();
        assert_eq!(
            second, 0,
            "second call must be no-op (idempotent); inserted {second} rows"
        );
        let issues_after_second = store.list_issues().await.unwrap().len();
        assert_eq!(
            issues_after_first, issues_after_second,
            "second call must not duplicate issues"
        );
    }

    #[tokio::test]
    async fn corpus_has_subject_variety_for_filter_testing() {
        // The D2 substring filter needs both match + no-match rows; the
        // corpus is shaped to give that. This test pins the property —
        // if a re-shuffle accidentally homogenizes the subjects, the
        // POC demo loses one of its talking points.
        let store = Store::open().await.unwrap();
        hydrate_demo_data(&store).await.unwrap();
        let subjects: Vec<String> = store
            .list_issues()
            .await
            .unwrap()
            .into_iter()
            .map(|i| i.subject.to_lowercase())
            .collect();
        // A common substring like "bug" should match SOME but not ALL.
        let bug_matches = subjects.iter().filter(|s| s.contains("bug")).count();
        assert!(
            bug_matches > 0 && bug_matches < subjects.len(),
            "expected `bug` to match some-but-not-all subjects (got {bug_matches}/{})",
            subjects.len()
        );
    }
}
