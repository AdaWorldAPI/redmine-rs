//! Taxonomy lookups (W5) — IssueStatus, Tracker, IssuePriority.
//!
//! Three near-identical admin lookup tables sharing the
//! `(name, position, is_<flag>)` shape. Codebook ids:
//!
//! | Concept | Class id | Redmine name | OP name |
//! |---|---|---|---|
//! | IssueStatus | `0x0105 project_status` | IssueStatus | Status |
//! | Tracker | `0x0106 project_type` | Tracker | Type |
//! | IssuePriority | `0x0107 priority` | IssuePriority | (Priority) |
//!
//! Each ports through `ogar_vocab::ports::*Port::class_id` to the
//! same canonical id, so Redmine `IssueStatus` and OpenProject
//! `Status` both render via `0x0105`.

use surrealdb_types::{RecordId, SurrealValue};

use crate::{Store, StoreError};

// ── IssueStatus ────────────────────────────────────────────────────

/// Input for [`Store::create_issue_status`].
#[derive(Debug, Clone, SurrealValue)]
pub struct NewIssueStatus {
    /// Status name (`"New"`, `"In Progress"`, `"Closed"`, …).
    pub name: String,
    /// Sort position.
    pub position: i64,
    /// Whether issues in this status count as "closed" for filtering /
    /// percent-done aggregation.
    pub is_closed: bool,
}

/// Row returned by [`Store::find_issue_status_by_name`] / list_issue_statuses.
#[derive(Debug, Clone, SurrealValue)]
pub struct IssueStatusRow {
    /// SurrealDB record id (`issue_status:<ulid>`).
    pub id: Option<RecordId>,
    /// Status name.
    pub name: String,
    /// Sort position.
    pub position: i64,
    /// True when this status counts as closed.
    pub is_closed: bool,
}

// ── Tracker ────────────────────────────────────────────────────────

/// Input for [`Store::create_tracker`].
#[derive(Debug, Clone, SurrealValue)]
pub struct NewTracker {
    /// Tracker name (`"Bug"`, `"Feature"`, `"Support"`, …).
    pub name: String,
    /// Sort position.
    pub position: i64,
    /// True for the tracker used when one isn't explicitly set on
    /// new issues.
    pub is_default: bool,
}

/// Row returned by [`Store::find_tracker_by_name`] / list_trackers.
#[derive(Debug, Clone, SurrealValue)]
pub struct TrackerRow {
    /// SurrealDB record id.
    pub id: Option<RecordId>,
    /// Tracker name.
    pub name: String,
    /// Sort position.
    pub position: i64,
    /// Default-tracker flag.
    pub is_default: bool,
}

// ── IssuePriority ──────────────────────────────────────────────────

/// Input for [`Store::create_issue_priority`].
#[derive(Debug, Clone, SurrealValue)]
pub struct NewIssuePriority {
    /// Priority name (`"Low"`, `"Normal"`, `"High"`, `"Urgent"`, …).
    pub name: String,
    /// Sort position.
    pub position: i64,
    /// True for the default-priority value applied to new issues.
    pub is_default: bool,
}

/// Row returned by [`Store::find_issue_priority_by_name`] / list_issue_priorities.
#[derive(Debug, Clone, SurrealValue)]
pub struct IssuePriorityRow {
    /// SurrealDB record id.
    pub id: Option<RecordId>,
    /// Priority name.
    pub name: String,
    /// Sort position.
    pub position: i64,
    /// Default-priority flag.
    pub is_default: bool,
}

// ── Store impls ────────────────────────────────────────────────────

impl Store {
    /// Insert an IssueStatus.
    pub async fn create_issue_status(
        &self,
        new: NewIssueStatus,
    ) -> Result<IssueStatusRow, StoreError> {
        let row: Option<IssueStatusRow> = self.db().create("issue_status").content(new).await?;
        row.ok_or(StoreError::NotFound)
    }

    /// Find an IssueStatus by name.
    pub async fn find_issue_status_by_name(
        &self,
        name: &str,
    ) -> Result<IssueStatusRow, StoreError> {
        find_by_name(self, "issue_status", name).await
    }

    /// List IssueStatuses.
    pub async fn list_issue_statuses(&self) -> Result<Vec<IssueStatusRow>, StoreError> {
        list_table(self, "issue_status").await
    }

    /// Insert a Tracker.
    pub async fn create_tracker(&self, new: NewTracker) -> Result<TrackerRow, StoreError> {
        let row: Option<TrackerRow> = self.db().create("tracker").content(new).await?;
        row.ok_or(StoreError::NotFound)
    }

    /// Find a Tracker by name.
    pub async fn find_tracker_by_name(&self, name: &str) -> Result<TrackerRow, StoreError> {
        find_by_name(self, "tracker", name).await
    }

    /// List Trackers.
    pub async fn list_trackers(&self) -> Result<Vec<TrackerRow>, StoreError> {
        list_table(self, "tracker").await
    }

    /// Insert an IssuePriority.
    pub async fn create_issue_priority(
        &self,
        new: NewIssuePriority,
    ) -> Result<IssuePriorityRow, StoreError> {
        let row: Option<IssuePriorityRow> = self.db().create("issue_priority").content(new).await?;
        row.ok_or(StoreError::NotFound)
    }

    /// Find an IssuePriority by name.
    pub async fn find_issue_priority_by_name(
        &self,
        name: &str,
    ) -> Result<IssuePriorityRow, StoreError> {
        find_by_name(self, "issue_priority", name).await
    }

    /// List IssuePriorities.
    pub async fn list_issue_priorities(&self) -> Result<Vec<IssuePriorityRow>, StoreError> {
        list_table(self, "issue_priority").await
    }
}

// ── Shared internals ───────────────────────────────────────────────

/// Generic "select first row WHERE name = $n" over a SurrealValue-shaped
/// row type. Three concrete callers share this; the W6+ taxonomy
/// candidates (Activity, EnumerationTimeEntryActivity, etc.) will too.
async fn find_by_name<T>(store: &Store, table: &'static str, name: &str) -> Result<T, StoreError>
where
    T: SurrealValue,
{
    match store
        .db()
        .query(format!("SELECT * FROM {table} WHERE name = $n LIMIT 1"))
        .bind(("n", name.to_string()))
        .await
    {
        Ok(mut res) => match res.take::<Vec<T>>(0) {
            Ok(rows) => rows.into_iter().next().ok_or(StoreError::NotFound),
            Err(e) if e.is_not_found() => Err(StoreError::NotFound),
            Err(e) => Err(StoreError::Surreal(e)),
        },
        Err(e) if e.is_not_found() => Err(StoreError::NotFound),
        Err(e) => Err(StoreError::Surreal(e)),
    }
}

async fn list_table<T>(store: &Store, table: &'static str) -> Result<Vec<T>, StoreError>
where
    T: SurrealValue,
{
    match store.db().select::<Vec<T>>(table).await {
        Ok(rows) => Ok(rows),
        Err(e) if e.is_not_found() => Ok(Vec::new()),
        Err(e) => Err(StoreError::Surreal(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn issue_status_round_trips() {
        let store = Store::open().await.unwrap();
        store
            .create_issue_status(NewIssueStatus {
                name: "Closed".to_string(),
                position: 99,
                is_closed: true,
            })
            .await
            .unwrap();
        let r = store.find_issue_status_by_name("Closed").await.unwrap();
        assert!(r.is_closed);
        assert_eq!(r.position, 99);
    }

    #[tokio::test]
    async fn tracker_round_trips() {
        let store = Store::open().await.unwrap();
        store
            .create_tracker(NewTracker {
                name: "Bug".to_string(),
                position: 1,
                is_default: true,
            })
            .await
            .unwrap();
        let r = store.find_tracker_by_name("Bug").await.unwrap();
        assert!(r.is_default);
    }

    #[tokio::test]
    async fn issue_priority_round_trips() {
        let store = Store::open().await.unwrap();
        store
            .create_issue_priority(NewIssuePriority {
                name: "Normal".to_string(),
                position: 2,
                is_default: true,
            })
            .await
            .unwrap();
        let r = store.find_issue_priority_by_name("Normal").await.unwrap();
        assert!(r.is_default);
    }

    #[tokio::test]
    async fn list_empty_then_populated_for_each_table() {
        let store = Store::open().await.unwrap();
        assert!(store.list_issue_statuses().await.unwrap().is_empty());
        assert!(store.list_trackers().await.unwrap().is_empty());
        assert!(store.list_issue_priorities().await.unwrap().is_empty());
        store
            .create_issue_status(NewIssueStatus {
                name: "New".to_string(),
                position: 1,
                is_closed: false,
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
                name: "Low".to_string(),
                position: 1,
                is_default: false,
            })
            .await
            .unwrap();
        assert_eq!(store.list_issue_statuses().await.unwrap().len(), 1);
        assert_eq!(store.list_trackers().await.unwrap().len(), 1);
        assert_eq!(store.list_issue_priorities().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn find_returns_not_found_for_unknown() {
        let store = Store::open().await.unwrap();
        assert!(matches!(
            store.find_issue_status_by_name("Nope").await,
            Err(StoreError::NotFound)
        ));
        assert!(matches!(
            store.find_tracker_by_name("Nope").await,
            Err(StoreError::NotFound)
        ));
        assert!(matches!(
            store.find_issue_priority_by_name("Nope").await,
            Err(StoreError::NotFound)
        ));
    }
}
