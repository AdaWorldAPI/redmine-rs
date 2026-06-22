//! IssueRelation (`project_relation` codebook id `0x0111`) —
//! work-item ↔ work-item dependency edges. W8 (relations half).
//!
//! The canonical class has two family edges (`from` / `to`
//! ProjectWorkItem) plus two attributes: `relation_type`
//! (`"precedes"`, `"blocks"`, `"relates"`, …) and `lag` (Redmine's
//! `delay` / OP's `lag` — offset in days, canonicalised to `lag`).
//!
//! The MVP row carries the two attributes; wiring the `from`/`to`
//! record links to actual Issue rows lands once the Issue detail
//! page embeds its relations (a W1-followup that needs the join the
//! canonical edges describe).

use surrealdb_types::{RecordId, SurrealValue};

use crate::{Store, StoreError};

/// Input for [`Store::create_relation`].
#[derive(Debug, Clone, SurrealValue)]
pub struct NewRelation {
    /// Relation kind: `"precedes"`, `"blocks"`, `"relates"`,
    /// `"duplicates"`, … (Redmine's `IssueRelation::TYPES`).
    pub relation_type: String,
    /// Offset in days between the two work items (Redmine `delay` /
    /// OpenProject `lag`, canonicalised to `lag`). `0` for
    /// non-scheduling relation types.
    pub lag: i64,
}

/// Row returned by [`Store::find_relation`] / [`Store::list_relations`].
#[derive(Debug, Clone, SurrealValue)]
pub struct RelationRow {
    /// SurrealDB record id (`relation:<ulid>`).
    pub id: Option<RecordId>,
    /// Relation kind.
    pub relation_type: String,
    /// Day offset.
    pub lag: i64,
}

impl Store {
    /// Insert an IssueRelation.
    pub async fn create_relation(&self, new: NewRelation) -> Result<RelationRow, StoreError> {
        let row: Option<RelationRow> = self.db().create("relation").content(new).await?;
        row.ok_or(StoreError::NotFound)
    }

    /// Read an IssueRelation by its SurrealDB record id. Relations
    /// have no natural slug (they're identified by their endpoints +
    /// type), so the URL keys on the record id like W1's Issue.
    pub async fn find_relation(&self, id: &RecordId) -> Result<RelationRow, StoreError> {
        match self.db().select(id.clone()).await {
            Ok(Some(row)) => Ok(row),
            Ok(None) => Err(StoreError::NotFound),
            Err(e) if e.is_not_found() => Err(StoreError::NotFound),
            Err(e) => Err(StoreError::Surreal(e)),
        }
    }

    /// List every IssueRelation, in insertion order.
    pub async fn list_relations(&self) -> Result<Vec<RelationRow>, StoreError> {
        match self.db().select::<Vec<RelationRow>>("relation").await {
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
    async fn create_then_find_round_trips() {
        let store = Store::open().await.unwrap();
        let inserted = store
            .create_relation(NewRelation {
                relation_type: "precedes".to_string(),
                lag: 2,
            })
            .await
            .unwrap();
        assert_eq!(inserted.relation_type, "precedes");
        assert_eq!(inserted.lag, 2);
        let id = inserted.id.clone().unwrap();
        let fetched = store.find_relation(&id).await.unwrap();
        assert_eq!(fetched.relation_type, "precedes");
        assert_eq!(fetched.lag, 2);
    }

    #[tokio::test]
    async fn find_not_found() {
        let store = Store::open().await.unwrap();
        let id = RecordId::new("relation", "missing");
        assert!(matches!(
            store.find_relation(&id).await,
            Err(StoreError::NotFound)
        ));
    }

    #[tokio::test]
    async fn list_empty_then_populated() {
        let store = Store::open().await.unwrap();
        assert!(store.list_relations().await.unwrap().is_empty());
        for t in ["precedes", "blocks"] {
            store
                .create_relation(NewRelation {
                    relation_type: t.to_string(),
                    lag: 0,
                })
                .await
                .unwrap();
        }
        assert_eq!(store.list_relations().await.unwrap().len(), 2);
    }
}
