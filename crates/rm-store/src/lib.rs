//! `rm-store` — SurrealDB persistence layer keyed by canonical OGAR
//! class_ids.
//!
//! W0.2 of the [Redmine Integration Plan][plan]. Boots a SurrealDB
//! instance, applies the OGAR-emitted schema for all 32 promoted
//! concepts, and exposes a minimal CRUD surface the Phase-1 width
//! tracks (W1..W8) compose into resource handlers.
//!
//! [plan]: https://github.com/AdaWorldAPI/OGAR/blob/main/docs/integration/REDMINE-INTEGRATION-PLAN.md
//!
//! # Where the schema comes from
//!
//! ```rust,ignore
//! use ogar_adapter_surrealql::emit_surrealql_ddl;
//! use ogar_vocab::all_promoted_classes;
//!
//! let ddl = emit_surrealql_ddl(&all_promoted_classes());
//! // → one DEFINE TABLE … DEFINE FIELD … block per canonical concept,
//! //   in class_ids::ALL order. The exact DDL is round-trip-pinned
//! //   by ogar-adapter-surrealql's test suite.
//! ```
//!
//! `Store::open()` applies that string in one `db.query(ddl)` call.
//! Schema drift is impossible across rm-* crates because everyone
//! reads from the same OGAR enumerator (`all_promoted_classes`).
//!
//! # Today's scope (W0.2 DoD)
//!
//! - In-memory SurrealDB (the `kv-mem` feature).
//! - Schema applied at startup.
//! - Insert + read-by-id for the headline concept (Issue /
//!   `project_work_item`), proving the round-trip works.
//!
//! # Deferred (later workstreams)
//!
//! - File-backed `Store::open_at(path)` once the rocksdb / surrealkv
//!   feature is needed for persistence.
//! - The generic `CRUD<T>` trait the Integration Plan §3 calls out —
//!   today's MVP exposes per-concept methods; the trait factors in
//!   when two concepts ship simultaneously (Plan §1.6 "three points
//!   form a line").
//! - SurrealDB-side index tuning for nested-set projects, full-text
//!   search (Plan D6).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod issue;
mod project;
mod role;
mod store;
mod time_entry;
mod user;

pub use error::StoreError;
pub use issue::{IssueRow, NewIssue};
pub use project::{NewProject, ProjectRow};
pub use role::{NewRole, RoleRow};
pub use store::Store;
pub use time_entry::{NewTimeEntry, TimeEntryRow};
pub use user::{NewUser, UserRow};
