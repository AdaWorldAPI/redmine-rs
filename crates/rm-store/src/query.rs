//! Query (`project_query` codebook id `0x010D`) — saved list-view
//! filter specs. W8 of the Redmine Integration Plan (saved-views half).
//!
//! The canonical class carries only `name` today; the actual filter /
//! column / sort payload Redmine stores (`filters`, `column_names`,
//! `sort_criteria`) lands when D2 (Plan §4 filters depth track) wires
//! the Query-as-data shape the harvest doc (`REDMINE-QUERY-HARVEST.md`)
//! mapped onto `RenderColumn`. For now a Query is a named saved view.

use surrealdb_types::{RecordId, SurrealValue};

use crate::{Store, StoreError};

/// Input for [`Store::create_query`].
#[derive(Debug, Clone, SurrealValue)]
pub struct NewQuery {
    /// Saved-view name (the URL slug + list-page label).
    pub name: String,
}

/// Row returned by [`Store::find_query_by_name`] / [`Store::list_queries`].
#[derive(Debug, Clone, SurrealValue)]
pub struct QueryRow {
    /// SurrealDB record id (`query:<ulid>`).
    pub id: Option<RecordId>,
    /// Saved-view name.
    pub name: String,
}

impl Store {
    /// Insert a Query.
    pub async fn create_query(&self, new: NewQuery) -> Result<QueryRow, StoreError> {
        let row: Option<QueryRow> = self.db().create("query").content(new).await?;
        row.ok_or(StoreError::NotFound)
    }

    /// Find a Query by name (the URL slug).
    pub async fn find_query_by_name(&self, name: &str) -> Result<QueryRow, StoreError> {
        match self
            .db()
            .query("SELECT * FROM query WHERE name = $n LIMIT 1")
            .bind(("n", name.to_string()))
            .await
        {
            Ok(mut res) => match res.take::<Vec<QueryRow>>(0) {
                Ok(rows) => rows.into_iter().next().ok_or(StoreError::NotFound),
                Err(e) if e.is_not_found() => Err(StoreError::NotFound),
                Err(e) => Err(StoreError::Surreal(e)),
            },
            Err(e) if e.is_not_found() => Err(StoreError::NotFound),
            Err(e) => Err(StoreError::Surreal(e)),
        }
    }

    /// List every Query.
    pub async fn list_queries(&self) -> Result<Vec<QueryRow>, StoreError> {
        match self.db().select::<Vec<QueryRow>>("query").await {
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
    async fn create_then_find_by_name() {
        let store = Store::open().await.unwrap();
        store
            .create_query(NewQuery {
                name: "Open bugs".to_string(),
            })
            .await
            .unwrap();
        let r = store.find_query_by_name("Open bugs").await.unwrap();
        assert_eq!(r.name, "Open bugs");
    }

    #[tokio::test]
    async fn find_not_found() {
        let store = Store::open().await.unwrap();
        assert!(matches!(
            store.find_query_by_name("Nope").await,
            Err(StoreError::NotFound)
        ));
    }

    #[tokio::test]
    async fn list_empty_then_populated() {
        let store = Store::open().await.unwrap();
        assert!(store.list_queries().await.unwrap().is_empty());
        for n in ["Open bugs", "My issues"] {
            store
                .create_query(NewQuery {
                    name: n.to_string(),
                })
                .await
                .unwrap();
        }
        assert_eq!(store.list_queries().await.unwrap().len(), 2);
    }
}
