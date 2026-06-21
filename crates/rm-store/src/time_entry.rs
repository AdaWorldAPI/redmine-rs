//! TimeEntry (`billable_work_entry` codebook id `0x0103`) CRUD.
//!
//! W3 of the Redmine Integration Plan, the time-tracking track.
//! Redmine + OpenProject both call this `TimeEntry`; the canonical
//! concept is `billable_work_entry`. Both ports converge on the same
//! class_id (`0x0103`) per `ogar_vocab::ports::*Port::class_id`.
//!
//! # Today's scope vs the canonical schema
//!
//! The OGAR canonical `billable_work_entry` declares one attribute
//! (`billable: boolean`) plus 12 family edges to upstream concepts
//! (`Worker`, `Duration`, `RatePolicy`, `TaxPolicy`, …) that aren't
//! yet promoted to top-level codebook classes. The MVP store row
//! below carries the Redmine + OpenProject runtime shape (`hours`,
//! `comments`, `spent_on`) NOT the canonical attribute set — and the
//! table name is the Rails-shape `time_entry` rather than the
//! canonical PascalCase `BillableWorkEntry`. This means the row is
//! stored in an undeclared (SCHEMALESS-default) sibling table, NOT
//! engaging the SCHEMAFULL DDL `ogar_adapter_surrealql::emit_surrealql_ddl`
//! emits at `Store::open`. Same divergence applies to W1
//! (`project_work_item` / `Issue`) and W2 (`project` / `Project`).
//!
//! **Follow-up sprint** — once the curator-vs-canonical schema
//! reconciliation lands (a sibling `ogar_vocab::*Port` extension
//! emitting the per-port DDL alongside the canonical one), each
//! resource's table name + row shape aligns with what's actually
//! enforced.

use surrealdb_types::{RecordId, SurrealValue};

use crate::{Store, StoreError};

/// Input for [`Store::create_time_entry`]. The Redmine-shape
/// minimum: hours + spent_on; comments is optional.
#[derive(Debug, Clone, SurrealValue)]
pub struct NewTimeEntry {
    /// Hours booked. Float so partial-hour entries round-trip
    /// (`0.25h` = 15 minutes — both ports support this).
    pub hours: f64,
    /// `YYYY-MM-DD` ISO 8601 date (the day work was performed).
    /// String today; a `Date` SurrealValue conversion lands when
    /// D2 needs to filter by date range.
    pub spent_on: String,
    /// Optional free-text comment.
    pub comments: Option<String>,
}

/// Row returned by [`Store::find_time_entry`] / [`Store::list_time_entries`].
#[derive(Debug, Clone, SurrealValue)]
pub struct TimeEntryRow {
    /// SurrealDB record id (`time_entry:<ulid>`).
    pub id: Option<RecordId>,
    /// Hours booked.
    pub hours: f64,
    /// `YYYY-MM-DD` spent-on date.
    pub spent_on: String,
    /// Optional comment.
    pub comments: Option<String>,
}

impl Store {
    /// Insert a TimeEntry. Returns the persisted row.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Surreal`] on driver failures.
    /// - [`StoreError::NotFound`] if CREATE returns no row.
    pub async fn create_time_entry(&self, new: NewTimeEntry) -> Result<TimeEntryRow, StoreError> {
        let row: Option<TimeEntryRow> = self.db().create("time_entry").content(new).await?;
        row.ok_or(StoreError::NotFound)
    }

    /// Read a TimeEntry by its record id.
    ///
    /// # Errors
    ///
    /// - [`StoreError::NotFound`] when the row is missing — covers
    ///   the SurrealDB-level NotFound (empty table) + the
    ///   rust-side `Option::None` shape.
    /// - [`StoreError::Surreal`] on driver failures.
    pub async fn find_time_entry(&self, id: &RecordId) -> Result<TimeEntryRow, StoreError> {
        match self.db().select(id.clone()).await {
            Ok(Some(row)) => Ok(row),
            Ok(None) => Err(StoreError::NotFound),
            Err(e) if e.is_not_found() => Err(StoreError::NotFound),
            Err(e) => Err(StoreError::Surreal(e)),
        }
    }

    /// List every TimeEntry, in insertion order. D2 (Plan §4) adds
    /// filter / sort / group / date-range.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Surreal`] on driver failures. Empty result
    ///   is `Ok(Vec::new())`, not an error.
    pub async fn list_time_entries(&self) -> Result<Vec<TimeEntryRow>, StoreError> {
        match self.db().select::<Vec<TimeEntryRow>>("time_entry").await {
            Ok(rows) => Ok(rows),
            Err(e) if e.is_not_found() => Ok(Vec::new()),
            Err(e) => Err(StoreError::Surreal(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_then_find_round_trips_a_time_entry() {
        let store = Store::open().await.unwrap();
        let new = NewTimeEntry {
            hours: 2.5,
            spent_on: "2026-06-21".to_string(),
            comments: Some("Worked on UnifiedBridge migration".to_string()),
        };
        let inserted = store.create_time_entry(new.clone()).await.unwrap();
        assert_eq!(inserted.hours, 2.5);
        assert_eq!(inserted.spent_on, "2026-06-21");
        assert_eq!(inserted.comments, new.comments);
        let id = inserted.id.clone().expect("inserted row carries an id");

        let fetched = store.find_time_entry(&id).await.unwrap();
        assert_eq!(fetched.id, Some(id));
        assert_eq!(fetched.hours, 2.5);
        assert_eq!(fetched.spent_on, "2026-06-21");
    }

    #[tokio::test]
    async fn find_time_entry_returns_not_found_for_missing_id() {
        let store = Store::open().await.unwrap();
        let id = RecordId::new("time_entry", "does_not_exist");
        let err = store.find_time_entry(&id).await.unwrap_err();
        assert!(matches!(err, StoreError::NotFound), "got {err:?}");
    }

    #[tokio::test]
    async fn list_time_entries_returns_empty_for_fresh_store() {
        let store = Store::open().await.unwrap();
        let rows = store.list_time_entries().await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn list_time_entries_returns_inserted_rows() {
        let store = Store::open().await.unwrap();
        for h in [1.0, 2.0, 0.5] {
            store
                .create_time_entry(NewTimeEntry {
                    hours: h,
                    spent_on: "2026-06-21".to_string(),
                    comments: None,
                })
                .await
                .unwrap();
        }
        let rows = store.list_time_entries().await.unwrap();
        assert_eq!(rows.len(), 3);
        let total: f64 = rows.iter().map(|r| r.hours).sum();
        assert!((total - 3.5).abs() < 1e-9, "hours sum got {total}");
    }

    #[tokio::test]
    async fn create_time_entry_without_comment_round_trips() {
        let store = Store::open().await.unwrap();
        let inserted = store
            .create_time_entry(NewTimeEntry {
                hours: 1.0,
                spent_on: "2026-06-21".to_string(),
                comments: None,
            })
            .await
            .unwrap();
        assert!(inserted.comments.is_none());
    }
}
