//! Issue (`project_work_item`) CRUD — the MVP slice that proves the
//! store round-trip works for the headline concept.
//!
//! Per the Integration Plan, the full generic `CRUD<T>` trait factors
//! in once a second concept lands (Plan §1.6 "three points form a
//! line"). Today the Issue surface is concrete; future per-resource
//! modules (W1..W8) follow this shape.

use surrealdb_types::{RecordId, SurrealValue};

use crate::{Store, StoreError};

/// Input shape for [`Store::create_issue`]. All required-on-creation
/// columns the OpenProject/Redmine flow needs to write a new Issue.
///
/// Maps onto `class_ids::PROJECT_WORK_ITEM` (`0x0102`). The full
/// attribute set on `ogar_vocab::project_work_item()` is wider; this
/// shape is just the "post a new issue from the create form" subset.
/// The detail-view route reads back via [`IssueRow`] which carries
/// every attribute.
#[derive(Debug, Clone, SurrealValue)]
pub struct NewIssue {
    /// Required — short one-line headline. Maps to the `subject`
    /// attribute on `project_work_item`.
    pub subject: String,
    /// Optional — long-form prose / markdown.
    pub description: Option<String>,
}

/// Row returned by [`Store::find_issue`] — the persisted shape.
///
/// `id` is the SurrealDB-assigned record id (`project_work_item:<ulid>`);
/// W1 (the Issue handler) extracts it for URL paths.
#[derive(Debug, Clone, SurrealValue)]
pub struct IssueRow {
    /// The SurrealDB record handle. `None` only inside a freshly
    /// constructed value that hasn't round-tripped through the store yet.
    pub id: Option<RecordId>,
    /// Required.
    pub subject: String,
    /// Optional.
    pub description: Option<String>,
}

impl Store {
    /// Insert a new Issue. Returns the persisted row (with its
    /// SurrealDB-assigned id filled in).
    ///
    /// # Errors
    ///
    /// - [`StoreError::Surreal`] if the CREATE query fails.
    /// - [`StoreError::NotFound`] if the CREATE returns no row
    ///   (defensive — shouldn't happen on a healthy instance).
    pub async fn create_issue(&self, new: NewIssue) -> Result<IssueRow, StoreError> {
        let row: Option<IssueRow> = self.db().create("project_work_item").content(new).await?;
        row.ok_or(StoreError::NotFound)
    }

    /// Read an Issue by its record id (e.g.
    /// `project_work_item:01HV…`). Returns
    /// [`StoreError::NotFound`] when the row doesn't exist.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Surreal`] on driver failures.
    /// - [`StoreError::NotFound`] when the row is missing — covers
    ///   both the SurrealDB-level not-found (empty table / missing
    ///   record id) and the rust-side `Option::None` shape that
    ///   `.select()` returns when the row genuinely isn't there.
    pub async fn find_issue(&self, id: &RecordId) -> Result<IssueRow, StoreError> {
        match self.db().select(id.clone()).await {
            Ok(Some(row)) => Ok(row),
            Ok(None) => Err(StoreError::NotFound),
            Err(e) if e.is_not_found() => Err(StoreError::NotFound),
            Err(e) => Err(StoreError::Surreal(e)),
        }
    }

    /// List every Issue in the store, in insertion order.
    ///
    /// MVP-shaped: no pagination, no filter / sort / group — those
    /// live in D2 (Plan §4 depth track). Today's list view renders
    /// whatever the store hands back.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Surreal`] on driver failures.
    /// - **Not** an error to return an empty Vec; the list page's
    ///   empty-state shows in that case (the askama kit pins
    ///   `"No data."` already).
    pub async fn list_issues(&self) -> Result<Vec<IssueRow>, StoreError> {
        match self.db().select::<Vec<IssueRow>>("project_work_item").await {
            Ok(rows) => Ok(rows),
            // Same "empty table = not-found" quirk find_issue handles
            // — for the list path that's "no rows yet", an empty
            // result, not an error.
            Err(e) if e.is_not_found() => Ok(Vec::new()),
            Err(e) => Err(StoreError::Surreal(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_then_find_round_trips_an_issue() {
        // The W0.2 DoD: insert an Issue, read it back.
        let store = Store::open().await.unwrap();
        let new = NewIssue {
            subject: "Smoke test issue".to_string(),
            description: Some("a description that should survive the round-trip".to_string()),
        };
        let inserted = store
            .create_issue(new.clone())
            .await
            .expect("create must succeed");
        assert_eq!(inserted.subject, new.subject);
        assert_eq!(inserted.description, new.description);
        let id = inserted.id.clone().expect("inserted row must carry an id");

        let fetched = store.find_issue(&id).await.expect("find must succeed");
        assert_eq!(fetched.id, Some(id));
        assert_eq!(fetched.subject, new.subject);
        assert_eq!(fetched.description, new.description);
    }

    #[tokio::test]
    async fn find_issue_returns_not_found_for_missing_id() {
        let store = Store::open().await.unwrap();
        let id = RecordId::new("project_work_item", "does_not_exist");
        let err = store.find_issue(&id).await.expect_err("must be err");
        assert!(matches!(err, StoreError::NotFound), "got {err:?}");
    }

    #[tokio::test]
    async fn create_issue_without_description_round_trips() {
        let store = Store::open().await.unwrap();
        let new = NewIssue {
            subject: "no description".to_string(),
            description: None,
        };
        let inserted = store.create_issue(new.clone()).await.unwrap();
        assert_eq!(inserted.subject, new.subject);
        assert!(inserted.description.is_none());
    }

    #[tokio::test]
    async fn list_issues_returns_empty_for_fresh_store() {
        let store = Store::open().await.unwrap();
        let rows = store.list_issues().await.expect("list must succeed");
        assert!(rows.is_empty(), "fresh store has no issues: got {rows:?}");
    }

    #[tokio::test]
    async fn list_issues_returns_inserted_rows() {
        let store = Store::open().await.unwrap();
        for i in 0..3 {
            store
                .create_issue(NewIssue {
                    subject: format!("issue {i}"),
                    description: None,
                })
                .await
                .unwrap();
        }
        let rows = store.list_issues().await.unwrap();
        assert_eq!(rows.len(), 3, "expected 3 issues, got {}", rows.len());
        let subjects: Vec<&str> = rows.iter().map(|r| r.subject.as_str()).collect();
        for i in 0..3 {
            let needle = format!("issue {i}");
            assert!(
                subjects.contains(&needle.as_str()),
                "missing `{needle}` in {subjects:?}"
            );
        }
    }
}
