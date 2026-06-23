//! `rm-handlers` — per-resource axum handlers for Redmine-RS.
//!
//! Phase 1 of the [Redmine Integration Plan][plan]. Width tracks
//! (W1..W8) each own one module here covering a single canonical
//! concept's list / detail (and later form) routes. Per Plan §8
//! file ownership: each module's file is the merge-conflict
//! boundary — parallel tracks never edit the same source file.
//!
//! [plan]: https://github.com/AdaWorldAPI/OGAR/blob/main/docs/integration/REDMINE-INTEGRATION-PLAN.md
//!
//! # Module map
//!
//! | Track | Concept(s) | Module |
//! |---|---|---|
//! | **W1** | `project_work_item` (Issue / WorkPackage) | [`issues`] |
//! | W2 (todo) | Project + Version + EnabledModule | `projects` |
//! | W3 (todo) | TimeEntry | `time_entries` |
//! | W4 (todo) | User + Membership + Role + MemberRole + Watcher | `users` etc. |
//! | … | | |
//!
//! # Shape every track follows
//!
//! ```rust,ignore
//! pub async fn list(State(state): State<AppState>) -> Result<Html<String>, AppError> {
//!     let class = ogar_vocab::<concept>();
//!     let cols = default_columns_for(&class);
//!     let rows = state.store.list_<resource>s().await?;
//!     let row_sources = rows.iter().map(|r| build_row(r, &cols)).collect::<Vec<_>>();
//!     let body = ogar_render_askama::render_list(/* … */)?;
//!     Ok(Html(wrap_in_doc(&body)))
//! }
//!
//! pub fn router(state: AppState) -> Router {
//!     Router::new()
//!         .route("/<resources>", get(list))
//!         .route("/<resources>/:id", get(detail))
//!         .with_state(state)
//! }
//! ```
//!
//! Once a second resource module lands the shared helpers
//! (`default_columns_for`, `build_row`, `wrap_in_doc`,
//! `record_id_to_u64`) factor out into a `common` module here.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod common;
pub mod issues;
pub mod list_chrome;
pub mod news;
pub mod projects;
pub mod queries;
pub mod relations;
pub mod roles;
pub mod scm;
pub mod taxonomy;
pub mod time_entries;
pub mod users;
pub mod wiki_pages;

pub use common::{
    encode_path_segment, html_escape, identifier_to_u64, record_id_to_u64, wrap_in_doc, AppState,
    HandlerError,
};
pub use list_chrome::{ListQuery, SortDir};
