//! SCM-light (W7) — Repository + Changeset row storage.
//!
//! Read-only metadata: the rows carry what Redmine stores ABOUT a
//! repository / commit, NOT a live VCS connection. The Git / SVN /
//! Mercurial driver layer (Redmine's `Repository::Git` etc.) is a
//! later sprint — W7 ships the browse surface over already-imported
//! metadata.
//!
//! | Concept | Class id | Canonical | Redmine model |
//! |---|---|---|---|
//! | Repository | `0x010A project_repository` | url + scm_type | Repository |
//! | Changeset  | `0x0112 project_changeset`  | revision + commit_date + comments | Changeset |

use surrealdb_types::{RecordId, SurrealValue};

use crate::{Store, StoreError};

// ── Repository (0x010A) ─────────────────────────────────────────────

/// Input for [`Store::create_repository`].
#[derive(Debug, Clone, SurrealValue)]
pub struct NewRepository {
    /// Clone / checkout URL.
    pub url: String,
    /// SCM kind: `"Git"`, `"Subversion"`, `"Mercurial"`, … (Redmine's
    /// `Repository#type` minus the `Repository::` prefix).
    pub scm_type: String,
}

/// Row returned by [`Store::find_repository`] / [`Store::list_repositories`].
#[derive(Debug, Clone, SurrealValue)]
pub struct RepositoryRow {
    /// SurrealDB record id (`repository:<ulid>`).
    pub id: Option<RecordId>,
    /// Clone / checkout URL.
    pub url: String,
    /// SCM kind.
    pub scm_type: String,
}

// ── Changeset (0x0112) ──────────────────────────────────────────────

/// Input for [`Store::create_changeset`].
#[derive(Debug, Clone, SurrealValue)]
pub struct NewChangeset {
    /// Revision identifier — git sha / svn revision number. The URL
    /// key (Redmine: `/repository/revisions/:rev`).
    pub revision: String,
    /// `YYYY-MM-DD` commit date (string today; a `Date` SurrealValue
    /// conversion lands when D2 needs date-range filtering).
    pub commit_date: String,
    /// Commit message.
    pub comments: String,
}

/// Row returned by [`Store::find_changeset_by_revision`] / [`Store::list_changesets`].
#[derive(Debug, Clone, SurrealValue)]
pub struct ChangesetRow {
    /// SurrealDB record id.
    pub id: Option<RecordId>,
    /// Revision identifier.
    pub revision: String,
    /// Commit date.
    pub commit_date: String,
    /// Commit message.
    pub comments: String,
}

impl Store {
    /// Insert a Repository.
    pub async fn create_repository(&self, new: NewRepository) -> Result<RepositoryRow, StoreError> {
        let row: Option<RepositoryRow> = self.db().create("repository").content(new).await?;
        row.ok_or(StoreError::NotFound)
    }

    /// Read a Repository by its SurrealDB record id (Redmine keys
    /// repositories by numeric PK, not a slug).
    pub async fn find_repository(&self, id: &RecordId) -> Result<RepositoryRow, StoreError> {
        match self.db().select(id.clone()).await {
            Ok(Some(row)) => Ok(row),
            Ok(None) => Err(StoreError::NotFound),
            Err(e) if e.is_not_found() => Err(StoreError::NotFound),
            Err(e) => Err(StoreError::Surreal(e)),
        }
    }

    /// List every Repository.
    pub async fn list_repositories(&self) -> Result<Vec<RepositoryRow>, StoreError> {
        match self.db().select::<Vec<RepositoryRow>>("repository").await {
            Ok(rows) => Ok(rows),
            Err(e) if e.is_not_found() => Ok(Vec::new()),
            Err(e) => Err(StoreError::Surreal(e)),
        }
    }

    /// Insert a Changeset.
    pub async fn create_changeset(&self, new: NewChangeset) -> Result<ChangesetRow, StoreError> {
        let row: Option<ChangesetRow> = self.db().create("changeset").content(new).await?;
        row.ok_or(StoreError::NotFound)
    }

    /// Find a Changeset by its revision identifier (the URL key).
    pub async fn find_changeset_by_revision(
        &self,
        revision: &str,
    ) -> Result<ChangesetRow, StoreError> {
        match self
            .db()
            .query("SELECT * FROM changeset WHERE revision = $r LIMIT 1")
            .bind(("r", revision.to_string()))
            .await
        {
            Ok(mut res) => match res.take::<Vec<ChangesetRow>>(0) {
                Ok(rows) => rows.into_iter().next().ok_or(StoreError::NotFound),
                Err(e) if e.is_not_found() => Err(StoreError::NotFound),
                Err(e) => Err(StoreError::Surreal(e)),
            },
            Err(e) if e.is_not_found() => Err(StoreError::NotFound),
            Err(e) => Err(StoreError::Surreal(e)),
        }
    }

    /// List every Changeset, in insertion order.
    pub async fn list_changesets(&self) -> Result<Vec<ChangesetRow>, StoreError> {
        match self.db().select::<Vec<ChangesetRow>>("changeset").await {
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
    async fn repository_round_trips() {
        let store = Store::open().await.unwrap();
        let inserted = store
            .create_repository(NewRepository {
                url: "https://github.com/AdaWorldAPI/redmine-rs.git".to_string(),
                scm_type: "Git".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(inserted.scm_type, "Git");
        let id = inserted.id.clone().unwrap();
        let fetched = store.find_repository(&id).await.unwrap();
        assert_eq!(fetched.url, inserted.url);
    }

    #[tokio::test]
    async fn repository_find_not_found() {
        let store = Store::open().await.unwrap();
        let id = RecordId::new("repository", "missing");
        assert!(matches!(
            store.find_repository(&id).await,
            Err(StoreError::NotFound)
        ));
    }

    #[tokio::test]
    async fn repository_list_empty_then_populated() {
        let store = Store::open().await.unwrap();
        assert!(store.list_repositories().await.unwrap().is_empty());
        store
            .create_repository(NewRepository {
                url: "u".to_string(),
                scm_type: "Git".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(store.list_repositories().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn changeset_round_trips_by_revision() {
        let store = Store::open().await.unwrap();
        store
            .create_changeset(NewChangeset {
                revision: "abc123".to_string(),
                commit_date: "2026-06-21".to_string(),
                comments: "Fix the foo".to_string(),
            })
            .await
            .unwrap();
        let r = store.find_changeset_by_revision("abc123").await.unwrap();
        assert_eq!(r.revision, "abc123");
        assert_eq!(r.comments, "Fix the foo");
    }

    #[tokio::test]
    async fn changeset_find_not_found() {
        let store = Store::open().await.unwrap();
        assert!(matches!(
            store.find_changeset_by_revision("nope").await,
            Err(StoreError::NotFound)
        ));
    }

    #[tokio::test]
    async fn changeset_list_empty_then_populated() {
        let store = Store::open().await.unwrap();
        assert!(store.list_changesets().await.unwrap().is_empty());
        for rev in ["r1", "r2"] {
            store
                .create_changeset(NewChangeset {
                    revision: rev.to_string(),
                    commit_date: "2026-06-21".to_string(),
                    comments: String::new(),
                })
                .await
                .unwrap();
        }
        assert_eq!(store.list_changesets().await.unwrap().len(), 2);
    }
}
