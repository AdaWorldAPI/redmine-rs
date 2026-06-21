//! WikiPage (`project_wiki_page` codebook id `0x010C`) — versioned
//! page content. The canonical class declares only `title`; the
//! body lives in a separate `WikiContent` table in Redmine + OP.
//! MVP collapses both into one row with a `body` field; versioning
//! lands when D-something history tracking ships.

use surrealdb_types::{RecordId, SurrealValue};

use crate::{Store, StoreError};

/// Input for [`Store::create_wiki_page`].
#[derive(Debug, Clone, SurrealValue)]
pub struct NewWikiPage {
    /// URL-safe page title (Redmine convention: title doubles as
    /// slug).
    pub title: String,
    /// Page body (Redmine wiki markup — Textile / Markdown depending
    /// on per-project setting; rendered as Plain today).
    pub body: String,
}

/// Row returned by [`Store::find_wiki_page_by_title`] / [`Store::list_wiki_pages`].
#[derive(Debug, Clone, SurrealValue)]
pub struct WikiPageRow {
    /// SurrealDB record id.
    pub id: Option<RecordId>,
    /// Title (URL slug).
    pub title: String,
    /// Page body.
    pub body: String,
}

impl Store {
    /// Insert a WikiPage.
    pub async fn create_wiki_page(&self, new: NewWikiPage) -> Result<WikiPageRow, StoreError> {
        let row: Option<WikiPageRow> = self.db().create("wiki_page").content(new).await?;
        row.ok_or(StoreError::NotFound)
    }

    /// Find a WikiPage by title (URL slug).
    pub async fn find_wiki_page_by_title(&self, title: &str) -> Result<WikiPageRow, StoreError> {
        match self
            .db()
            .query("SELECT * FROM wiki_page WHERE title = $t LIMIT 1")
            .bind(("t", title.to_string()))
            .await
        {
            Ok(mut res) => match res.take::<Vec<WikiPageRow>>(0) {
                Ok(rows) => rows.into_iter().next().ok_or(StoreError::NotFound),
                Err(e) if e.is_not_found() => Err(StoreError::NotFound),
                Err(e) => Err(StoreError::Surreal(e)),
            },
            Err(e) if e.is_not_found() => Err(StoreError::NotFound),
            Err(e) => Err(StoreError::Surreal(e)),
        }
    }

    /// List every WikiPage.
    pub async fn list_wiki_pages(&self) -> Result<Vec<WikiPageRow>, StoreError> {
        match self.db().select::<Vec<WikiPageRow>>("wiki_page").await {
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
    async fn create_then_find_by_title() {
        let store = Store::open().await.unwrap();
        store
            .create_wiki_page(NewWikiPage {
                title: "Home".to_string(),
                body: "Welcome.".to_string(),
            })
            .await
            .unwrap();
        let r = store.find_wiki_page_by_title("Home").await.unwrap();
        assert_eq!(r.title, "Home");
        assert_eq!(r.body, "Welcome.");
    }

    #[tokio::test]
    async fn find_returns_not_found() {
        let store = Store::open().await.unwrap();
        assert!(matches!(
            store.find_wiki_page_by_title("Nope").await,
            Err(StoreError::NotFound)
        ));
    }

    #[tokio::test]
    async fn list_empty_then_populated() {
        let store = Store::open().await.unwrap();
        assert!(store.list_wiki_pages().await.unwrap().is_empty());
        for t in ["Home", "Guide"] {
            store
                .create_wiki_page(NewWikiPage {
                    title: t.to_string(),
                    body: String::new(),
                })
                .await
                .unwrap();
        }
        assert_eq!(store.list_wiki_pages().await.unwrap().len(), 2);
    }
}
