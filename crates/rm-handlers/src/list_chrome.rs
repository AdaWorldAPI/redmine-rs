//! **D2** — list-view chrome: filter bar, clickable sort headers, and
//! pagination strip. The "17 years of Redmine" UX layer wrapping the
//! canonical [`render_list`] output.
//!
//! [`render_list`]: ogar_render_askama::render_list
//!
//! Today's scope is the minimum honest slice on the existing data shape:
//!
//! - **`?q=`** — case-insensitive substring filter on a single configurable
//!   text field (subject, name, …). Redmine's quick-search shape, the most
//!   commonly-used filter across 17 years of the issue list.
//! - **`?sort=<col>[:asc|desc]`** — sort by a named column with explicit
//!   direction. Render kit's `sortable` flag is now URL-emitted: each
//!   sortable header is a link that toggles direction.
//! - **`?page=N&per_page=N`** — 1-indexed page over the filtered+sorted
//!   set. `per_page` defaults to 25 (Redmine's default), capped at 100 so
//!   a hostile URL can't force the whole table into one render.
//!
//! Status / priority / tracker / assignee facets are NOT covered today —
//! the IssueRow doesn't carry those foreign keys yet (W4 actor + W2/W3
//! taxonomy land next). When they do, this module grows one helper per
//! facet without re-deriving the pagination math.
//!
//! Designed as a generic helper from day one so the **W2 (projects), W3
//! (time-entries), W4 (users / members), and later resources reuse the
//! same chrome** instead of each re-rolling its own search box.

use std::fmt::Write as _;

use serde::Deserialize;

use crate::common::html_escape;

/// Query-string parameters every D2-enabled list route accepts. Each field
/// is optional so a bare `GET /issues` still works (no query params = the
/// first page, default sort, no filter).
///
/// Use with axum's `Query<ListQuery>` extractor:
///
/// ```ignore
/// pub async fn list(
///     State(state): State<AppState>,
///     Query(q): Query<ListQuery>,
/// ) -> Result<Html<String>, HandlerError> { ... }
/// ```
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    /// Substring filter (URL `?q=foo`). The handler decides which text
    /// field(s) to match against — usually the row's primary text column
    /// (subject for issues, name for projects, etc.).
    #[serde(default)]
    pub q: Option<String>,
    /// Sort spec, `"<col>"` or `"<col>:asc"` / `"<col>:desc"`. The handler
    /// validates `<col>` against its column allow-list — unknown columns
    /// fall back silently to the resource's default order (insertion).
    #[serde(default)]
    pub sort: Option<String>,
    /// 1-indexed page. Values `< 1` clamp to `1` via [`Self::page`].
    #[serde(default)]
    pub page: Option<u32>,
    /// Items per page. Values `< 1` or `> 100` clamp via [`Self::per_page`]
    /// to guard against a hostile URL forcing a huge render.
    #[serde(default)]
    pub per_page: Option<u32>,
}

/// Sort direction parsed from a `<col>:asc|desc` suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// Ascending — the default when the suffix is omitted or unrecognized.
    Asc,
    /// Descending — when the suffix is `:desc`.
    Desc,
}

impl SortDir {
    /// Toggle direction — the column-header click semantic.
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }

    /// `"asc"` / `"desc"` — the URL spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

impl ListQuery {
    /// The substring filter, lowercased and trimmed for case-insensitive
    /// comparison. Empty string ⇒ no filter.
    #[must_use]
    pub fn search_needle(&self) -> String {
        self.q
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase)
            .unwrap_or_default()
    }

    /// 1-indexed page number, clamped to `>= 1`.
    #[must_use]
    pub fn page(&self) -> u32 {
        self.page.unwrap_or(1).max(1)
    }

    /// Items per page. Defaults to Redmine's `25`; clamped to `1..=100` so
    /// a hostile URL can't ask for the whole table at once.
    #[must_use]
    pub fn per_page(&self) -> u32 {
        self.per_page.unwrap_or(25).clamp(1, 100)
    }

    /// Parse `?sort=` into `(column, direction)`. `None` when no sort is
    /// requested or the value can't be parsed. `:asc` / `:desc` recognised;
    /// anything else after the colon falls back to `Asc`.
    #[must_use]
    pub fn sort(&self) -> Option<(&str, SortDir)> {
        let spec = self.sort.as_deref()?.trim();
        if spec.is_empty() {
            return None;
        }
        match spec.split_once(':') {
            Some((col, "desc")) => Some((col.trim(), SortDir::Desc)),
            Some((col, _)) => Some((col.trim(), SortDir::Asc)),
            None => Some((spec, SortDir::Asc)),
        }
    }

    /// Compute the page slice indices `[start, end)` over a collection of
    /// `total` items already in display order. `start == total` when the
    /// requested page is past the end (the slice is then empty, never out
    /// of bounds).
    #[must_use]
    pub fn page_window(&self, total: usize) -> (usize, usize) {
        let per = self.per_page() as usize;
        let start = ((self.page() - 1) as usize).saturating_mul(per).min(total);
        let end = start.saturating_add(per).min(total);
        (start, end)
    }

    /// Render the search input + hidden state inputs as one form. Submits
    /// to `action` (the list URL). Hidden inputs preserve sort/per_page so
    /// the user keeps their view; `page` resets to 1 on a new filter.
    #[must_use]
    pub fn render_filter_bar(&self, action: &str, placeholder: &str) -> String {
        let action_esc = html_escape(action);
        let needle = html_escape(self.q.as_deref().unwrap_or(""));
        let placeholder_esc = html_escape(placeholder);
        let mut out = String::with_capacity(256);
        let _ = write!(
            &mut out,
            r#"<form class="list-filter" method="get" action="{action_esc}" role="search">
  <input type="search" name="q" value="{needle}" placeholder="{placeholder_esc}" aria-label="Filter">"#
        );
        // `.filter(|s| !s.is_empty())` collapses the empty-check into the
        // pattern so clippy's `collapsible_if` doesn't fire on a nested
        // `if let Some(_) { if !is_empty { ... } }` (workspace is edition
        // 2021 — let-chains aren't available here).
        if let Some(spec) = self.sort.as_deref().filter(|s| !s.is_empty()) {
            let _ = write!(
                &mut out,
                r#"<input type="hidden" name="sort" value="{}">"#,
                html_escape(spec)
            );
        }
        // Preserve the explicit per_page override; omit when default (25).
        if let Some(per_page) = self.per_page.filter(|p| *p != 25) {
            let _ = write!(
                &mut out,
                r#"<input type="hidden" name="per_page" value="{per_page}">"#
            );
        }
        out.push_str(r#"<button type="submit">Apply</button>"#);
        if !needle.is_empty() {
            // "Clear" link that drops just `q`, keeps sort/per_page.
            let cleared = Self {
                q: None,
                sort: self.sort.clone(),
                page: None,
                per_page: self.per_page,
            };
            let _ = write!(
                &mut out,
                r#"<a class="filter-clear" href="{}">Clear</a>"#,
                cleared.as_query_path(action)
            );
        }
        out.push_str("</form>");
        out
    }

    /// Build an `href` for a sortable column header. Clicking toggles the
    /// direction when this column is already active; otherwise sorts asc
    /// by default. Always resets to page 1 (the Redmine convention — a
    /// new sort starts from the top of the result).
    #[must_use]
    pub fn sort_href(&self, action: &str, column: &str) -> String {
        let (next_col, next_dir) = match self.sort() {
            Some((active, dir)) if active == column => (column, dir.toggled()),
            _ => (column, SortDir::Asc),
        };
        let next = Self {
            q: self.q.clone(),
            sort: Some(format!("{next_col}:{}", next_dir.as_str())),
            page: None,
            per_page: self.per_page,
        };
        next.as_query_path(action)
    }

    /// Build the pagination strip: `Prev` / `Next` plus the current
    /// position `"page of total_pages"`. Minimalist (no per-page jumpers)
    /// — Redmine's modern shape with the same data Redmine 5.0 shows.
    /// Empty string when `total_items == 0` (the empty-state message in
    /// the table already conveys "no results").
    #[must_use]
    pub fn render_pagination(&self, action: &str, total_items: usize) -> String {
        if total_items == 0 {
            return String::new();
        }
        let per = self.per_page() as usize;
        let total_pages = total_items.div_ceil(per).max(1);
        let cur = (self.page() as usize).min(total_pages);
        let mut out = String::with_capacity(160);
        out.push_str(r#"<nav class="list-pagination" aria-label="Pagination">"#);
        if cur > 1 {
            let prev = Self {
                q: self.q.clone(),
                sort: self.sort.clone(),
                page: Some(cur as u32 - 1),
                per_page: self.per_page,
            };
            let _ = write!(
                &mut out,
                r#"<a rel="prev" href="{}">« Prev</a>"#,
                prev.as_query_path(action)
            );
        }
        let _ = write!(
            &mut out,
            r#"<span class="page-position">Page {cur} of {total_pages} ({total_items})</span>"#
        );
        if cur < total_pages {
            let next = Self {
                q: self.q.clone(),
                sort: self.sort.clone(),
                page: Some(cur as u32 + 1),
                per_page: self.per_page,
            };
            let _ = write!(
                &mut out,
                r#"<a rel="next" href="{}">Next »</a>"#,
                next.as_query_path(action)
            );
        }
        out.push_str("</nav>");
        out
    }

    /// Render `action?<canonical query string>` with only the set fields,
    /// each value HTML-escaped (the href lives in an HTML attribute).
    /// Used by [`Self::render_filter_bar`], [`Self::sort_href`],
    /// [`Self::render_pagination`].
    fn as_query_path(&self, action: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        // `.filter()` collapses each predicate into the pattern so clippy's
        // `collapsible_if` doesn't fire (edition 2021 — no let-chains).
        if let Some(q) = self.q.as_deref().filter(|q| !q.is_empty()) {
            parts.push(format!("q={}", percent_encode(q)));
        }
        if let Some(s) = self.sort.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("sort={}", percent_encode(s)));
        }
        if let Some(p) = self.page.filter(|p| *p > 1) {
            parts.push(format!("page={p}"));
        }
        if let Some(pp) = self.per_page.filter(|p| *p != 25) {
            parts.push(format!("per_page={pp}"));
        }
        if parts.is_empty() {
            html_escape(action)
        } else {
            format!("{}?{}", html_escape(action), parts.join("&amp;"))
        }
    }
}

/// Percent-encode a single query-string value. Conservative: anything
/// outside the URL-unreserved set (`A-Z a-z 0-9 - . _ ~`) is `%HH`-escaped.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => {
                let _ = write!(&mut out, "%{b:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_query_yields_first_page_no_filter_no_sort() {
        let q = ListQuery::default();
        assert_eq!(q.search_needle(), "");
        assert_eq!(q.page(), 1);
        assert_eq!(q.per_page(), 25);
        assert_eq!(q.sort(), None);
        assert_eq!(q.page_window(0), (0, 0));
        assert_eq!(q.page_window(100), (0, 25));
    }

    #[test]
    fn search_needle_trims_and_lowercases_only_non_empty_values() {
        let q = ListQuery {
            q: Some("  Foo BAR  ".into()),
            ..Default::default()
        };
        assert_eq!(q.search_needle(), "foo bar");
        let blank = ListQuery {
            q: Some("   ".into()),
            ..Default::default()
        };
        assert_eq!(blank.search_needle(), "");
    }

    #[test]
    fn per_page_clamps_to_redmine_band() {
        // Below floor → 1; above ceiling → 100; default → 25.
        assert_eq!(
            ListQuery {
                per_page: Some(0),
                ..Default::default()
            }
            .per_page(),
            1
        );
        assert_eq!(
            ListQuery {
                per_page: Some(50_000),
                ..Default::default()
            }
            .per_page(),
            100
        );
        assert_eq!(
            ListQuery {
                per_page: Some(50),
                ..Default::default()
            }
            .per_page(),
            50
        );
    }

    #[test]
    fn page_clamps_to_at_least_one() {
        assert_eq!(
            ListQuery {
                page: Some(0),
                ..Default::default()
            }
            .page(),
            1
        );
        assert_eq!(
            ListQuery {
                page: Some(7),
                ..Default::default()
            }
            .page(),
            7
        );
    }

    #[test]
    fn sort_parses_column_and_direction() {
        let q = ListQuery {
            sort: Some("subject".into()),
            ..Default::default()
        };
        assert_eq!(q.sort(), Some(("subject", SortDir::Asc)));
        let q = ListQuery {
            sort: Some("subject:desc".into()),
            ..Default::default()
        };
        assert_eq!(q.sort(), Some(("subject", SortDir::Desc)));
        let q = ListQuery {
            sort: Some("subject:asc".into()),
            ..Default::default()
        };
        assert_eq!(q.sort(), Some(("subject", SortDir::Asc)));
        let q = ListQuery {
            sort: Some("subject:gibberish".into()),
            ..Default::default()
        };
        // Unknown suffix falls back to asc — the safe default.
        assert_eq!(q.sort(), Some(("subject", SortDir::Asc)));
        let q = ListQuery {
            sort: Some("".into()),
            ..Default::default()
        };
        assert_eq!(q.sort(), None);
    }

    #[test]
    fn page_window_is_never_out_of_bounds() {
        // Page 1: [0, 25); page 2 with 30 items: [25, 30); page 9999: empty.
        let q1 = ListQuery::default();
        assert_eq!(q1.page_window(30), (0, 25));
        let q2 = ListQuery {
            page: Some(2),
            ..Default::default()
        };
        assert_eq!(q2.page_window(30), (25, 30));
        let q9 = ListQuery {
            page: Some(9999),
            ..Default::default()
        };
        let (s, e) = q9.page_window(30);
        assert!(s <= 30 && e <= 30 && s <= e, "got ({s},{e}) total=30");
    }

    #[test]
    fn sort_href_toggles_direction_on_same_column() {
        // First click on subject when no sort active → asc.
        let q0 = ListQuery::default();
        let href = q0.sort_href("/issues", "subject");
        assert!(href.contains("sort=subject%3Aasc"), "{href}");
        // Already asc on subject → toggles to desc.
        let q1 = ListQuery {
            sort: Some("subject:asc".into()),
            ..Default::default()
        };
        let href = q1.sort_href("/issues", "subject");
        assert!(href.contains("sort=subject%3Adesc"), "{href}");
        // desc on subject → toggles to asc.
        let q2 = ListQuery {
            sort: Some("subject:desc".into()),
            ..Default::default()
        };
        let href = q2.sort_href("/issues", "subject");
        assert!(href.contains("sort=subject%3Aasc"), "{href}");
    }

    #[test]
    fn sort_href_resets_page_so_a_new_sort_starts_from_the_top() {
        // Operator convention — switching sort jumps back to page 1.
        let q = ListQuery {
            sort: Some("subject:asc".into()),
            page: Some(7),
            ..Default::default()
        };
        let href = q.sort_href("/issues", "subject");
        assert!(
            !href.contains("page="),
            "page must reset on new sort: {href}"
        );
    }

    #[test]
    fn filter_bar_preserves_sort_and_renders_clear_link_when_filtered() {
        let q = ListQuery {
            q: Some("foo".into()),
            sort: Some("subject:desc".into()),
            ..Default::default()
        };
        let bar = q.render_filter_bar("/issues", "Filter issues");
        assert!(bar.contains(r#"action="/issues""#));
        assert!(bar.contains(r#"value="foo""#));
        assert!(bar.contains(r#"placeholder="Filter issues""#));
        // Sort preserved as a hidden input across the filter submit.
        assert!(bar.contains(r#"name="sort" value="subject:desc""#));
        // Clear link visible when filter is active.
        assert!(bar.contains("Clear"));
    }

    #[test]
    fn pagination_emits_prev_next_and_position() {
        let q = ListQuery {
            page: Some(2),
            ..Default::default()
        };
        let nav = q.render_pagination("/issues", 80);
        assert!(nav.contains("Prev"), "{nav}");
        assert!(nav.contains("Page 2 of 4 (80)"), "{nav}");
        assert!(nav.contains("Next"), "{nav}");
        // Prev links back to page 1 — the canonical default, so the URL
        // is just the action (no `?page=1` parameter; matches Redmine's
        // own URL canonicalization).
        assert!(nav.contains(r#"href="/issues""#), "{nav}");
        // Next links to page 3 explicitly.
        assert!(nav.contains(r#"href="/issues?page=3""#), "{nav}");
    }

    #[test]
    fn pagination_preserves_filter_and_sort_state_across_pages() {
        let q = ListQuery {
            q: Some("foo".into()),
            sort: Some("subject:desc".into()),
            page: Some(2),
            per_page: Some(50),
        };
        let nav = q.render_pagination("/issues", 200);
        // Both Prev (→ page 1, omitted since canonical) and Next (→ page 3)
        // must keep q + sort + per_page in the URL so the filtered/sorted
        // view survives navigation.
        for marker in ["q=foo", "sort=subject%3Adesc", "per_page=50"] {
            assert!(
                nav.matches(marker).count() >= 2,
                "expected `{marker}` to appear in BOTH prev + next hrefs:\n{nav}"
            );
        }
    }

    #[test]
    fn pagination_is_empty_when_no_items() {
        let nav = ListQuery::default().render_pagination("/issues", 0);
        assert!(nav.is_empty(), "no nav for empty result, got: {nav:?}");
    }

    #[test]
    fn query_path_escapes_html_in_action_and_percent_encodes_values() {
        let q = ListQuery {
            q: Some("a&b=c".into()),
            ..Default::default()
        };
        let href = q.sort_href(r#"/issues"><script>"#, "subject");
        // Action HTML-escaped (no raw `<` survives).
        assert!(!href.contains("<script>"), "{href}");
        // The q= value percent-encoded — `&` → %26, `=` → %3D.
        assert!(href.contains("q=a%26b%3Dc"), "{href}");
    }

    #[test]
    fn percent_encode_passes_unreserved_unchanged() {
        assert_eq!(percent_encode("Abc-_.~123"), "Abc-_.~123");
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("&=?#"), "%26%3D%3F%23");
    }
}
