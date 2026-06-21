//! News (`project_news` codebook id `0x0114`) — project-scoped
//! announcement / changelog entries.
//!
//! W6 of the Redmine Integration Plan, first comms-track concept.

use surrealdb_types::{RecordId, SurrealValue};

use crate::{Store, StoreError};

/// Input for [`Store::create_news`].
#[derive(Debug, Clone, SurrealValue)]
pub struct NewNews {
    /// Headline (renders as the link label on the list page).
    pub title: String,
    /// One-line summary shown next to the title.
    pub summary: String,
    /// Long-form body (markdown — rendered as Plain today, RichText
    /// once D1 forms + a markdown renderer land).
    pub description: String,
}

/// Row returned by [`Store::find_news`] / [`Store::list_news`].
#[derive(Debug, Clone, SurrealValue)]
pub struct NewsRow {
    /// SurrealDB record id (`news:<ulid>`).
    pub id: Option<RecordId>,
    /// Headline.
    pub title: String,
    /// One-line summary.
    pub summary: String,
    /// Long-form body.
    pub description: String,
}

impl Store {
    /// Insert a News entry.
    pub async fn create_news(&self, new: NewNews) -> Result<NewsRow, StoreError> {
        let row: Option<NewsRow> = self.db().create("news").content(new).await?;
        row.ok_or(StoreError::NotFound)
    }

    /// Read a News entry by its SurrealDB record id.
    pub async fn find_news(&self, id: &RecordId) -> Result<NewsRow, StoreError> {
        match self.db().select(id.clone()).await {
            Ok(Some(row)) => Ok(row),
            Ok(None) => Err(StoreError::NotFound),
            Err(e) if e.is_not_found() => Err(StoreError::NotFound),
            Err(e) => Err(StoreError::Surreal(e)),
        }
    }

    /// List every News entry, in insertion order.
    pub async fn list_news(&self) -> Result<Vec<NewsRow>, StoreError> {
        match self.db().select::<Vec<NewsRow>>("news").await {
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
        let new = NewNews {
            title: "Release 0.1".to_string(),
            summary: "First MVP cut".to_string(),
            description: "Browse + create + auth landed.".to_string(),
        };
        let inserted = store.create_news(new.clone()).await.unwrap();
        assert_eq!(inserted.title, new.title);
        let id = inserted.id.clone().unwrap();
        let fetched = store.find_news(&id).await.unwrap();
        assert_eq!(fetched.title, new.title);
        assert_eq!(fetched.summary, new.summary);
    }

    #[tokio::test]
    async fn find_returns_not_found() {
        let store = Store::open().await.unwrap();
        let id = RecordId::new("news", "missing");
        assert!(matches!(
            store.find_news(&id).await,
            Err(StoreError::NotFound)
        ));
    }

    #[tokio::test]
    async fn list_empty_then_populated() {
        let store = Store::open().await.unwrap();
        assert!(store.list_news().await.unwrap().is_empty());
        for i in 0..2 {
            store
                .create_news(NewNews {
                    title: format!("entry {i}"),
                    summary: String::new(),
                    description: String::new(),
                })
                .await
                .unwrap();
        }
        assert_eq!(store.list_news().await.unwrap().len(), 2);
    }
}
