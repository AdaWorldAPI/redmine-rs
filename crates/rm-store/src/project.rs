//! Project (`project` codebook id `0x0101`) CRUD — the container
//! concept that every Issue / TimeEntry belongs to.
//!
//! Mirrors the [`crate::issue`] module shape (W1 → W2 — "three points
//! form a line" lands on W3, factoring is for then). Where Issue is
//! looked up by SurrealDB record key, Project is looked up by its
//! Redmine-shape **identifier** slug (the URL `/projects/<identifier>`
//! convention every Redmine deployment uses).

use surrealdb_types::{RecordId, SurrealValue};

use crate::{Store, StoreError};

/// Input for [`Store::create_project`]. Both fields required to
/// satisfy the `project` SCHEMAFULL DDL emitted from OGAR.
#[derive(Debug, Clone, SurrealValue)]
pub struct NewProject {
    /// Human-readable name (displayed in lists / nav).
    pub name: String,
    /// URL-safe slug — unique. Matches Redmine's `Project#identifier`
    /// + OpenProject's `Project#identifier`. Used as the routing key.
    pub identifier: String,
}

/// Row returned by [`Store::find_project`] / [`Store::list_projects`].
#[derive(Debug, Clone, SurrealValue)]
pub struct ProjectRow {
    /// SurrealDB record id (`project:<ulid>`). Populated post-insert.
    pub id: Option<RecordId>,
    /// Human-readable name.
    pub name: String,
    /// URL slug.
    pub identifier: String,
}

impl Store {
    /// Insert a Project. Returns the persisted row.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Surreal`] on driver failures (including
    ///   `identifier` uniqueness conflicts once an index lands).
    /// - [`StoreError::NotFound`] if CREATE returns no row.
    pub async fn create_project(&self, new: NewProject) -> Result<ProjectRow, StoreError> {
        let row: Option<ProjectRow> = self.db().create("project").content(new).await?;
        row.ok_or(StoreError::NotFound)
    }

    /// Find a Project by its Redmine-shape **identifier** slug — the
    /// URL key (e.g. `/projects/my-project` looks up
    /// `identifier = "my-project"`).
    ///
    /// # Errors
    ///
    /// - [`StoreError::NotFound`] when no row matches.
    /// - [`StoreError::Surreal`] on driver failures.
    pub async fn find_project_by_identifier(
        &self,
        identifier: &str,
    ) -> Result<ProjectRow, StoreError> {
        match self
            .db()
            .query("SELECT * FROM project WHERE identifier = $ident LIMIT 1")
            .bind(("ident", identifier.to_string()))
            .await
        {
            Ok(mut res) => match res.take::<Vec<ProjectRow>>(0) {
                Ok(rows) => rows.into_iter().next().ok_or(StoreError::NotFound),
                Err(e) if e.is_not_found() => Err(StoreError::NotFound),
                Err(e) => Err(StoreError::Surreal(e)),
            },
            Err(e) if e.is_not_found() => Err(StoreError::NotFound),
            Err(e) => Err(StoreError::Surreal(e)),
        }
    }

    /// List every Project in the store, in insertion order. No
    /// pagination / sort / filter today (D2 — Plan §4).
    ///
    /// # Errors
    ///
    /// - [`StoreError::Surreal`] on driver failures. Empty result is
    ///   `Ok(Vec::new())`, not an error.
    pub async fn list_projects(&self) -> Result<Vec<ProjectRow>, StoreError> {
        match self.db().select::<Vec<ProjectRow>>("project").await {
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
    async fn create_then_find_by_identifier_round_trips() {
        let store = Store::open().await.unwrap();
        let new = NewProject {
            name: "My Project".to_string(),
            identifier: "my-project".to_string(),
        };
        let inserted = store.create_project(new.clone()).await.unwrap();
        assert_eq!(inserted.name, new.name);
        assert_eq!(inserted.identifier, new.identifier);
        let fetched = store
            .find_project_by_identifier("my-project")
            .await
            .unwrap();
        assert_eq!(fetched.identifier, "my-project");
        assert_eq!(fetched.name, "My Project");
    }

    #[tokio::test]
    async fn find_by_identifier_returns_not_found_for_unknown_slug() {
        let store = Store::open().await.unwrap();
        let err = store
            .find_project_by_identifier("nope")
            .await
            .expect_err("expected NotFound");
        assert!(matches!(err, StoreError::NotFound), "got {err:?}");
    }

    #[tokio::test]
    async fn list_projects_returns_empty_for_fresh_store() {
        let store = Store::open().await.unwrap();
        let rows = store.list_projects().await.unwrap();
        assert!(rows.is_empty(), "fresh store has no projects: {rows:?}");
    }

    #[tokio::test]
    async fn list_projects_returns_inserted_rows() {
        let store = Store::open().await.unwrap();
        for (name, ident) in [("Alpha", "alpha"), ("Beta", "beta"), ("Gamma", "gamma")] {
            store
                .create_project(NewProject {
                    name: name.to_string(),
                    identifier: ident.to_string(),
                })
                .await
                .unwrap();
        }
        let rows = store.list_projects().await.unwrap();
        assert_eq!(rows.len(), 3);
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        for n in ["Alpha", "Beta", "Gamma"] {
            assert!(names.contains(&n), "missing `{n}` in {names:?}");
        }
    }
}
