//! User (`project_actor` codebook id `0x0104`) CRUD — the actor
//! identity, Redmine + OpenProject converge here via the STI fold
//! (`User` / `Principal` / `Group` all → `PROJECT_ACTOR`, per
//! `ogar_vocab::ports::*Port::class_id` →
//! `class_ids::PROJECT_ACTOR`).
//!
//! W4 of the Redmine Integration Plan, first concept in the
//! actors-and-access track.
//!
//! # Today's scope
//!
//! - In-store user shape carries `login` + `display_name` (no
//!   password — `rm-auth`'s seed users still drive login; a sibling
//!   `rm_credentials` table with argon2 hashes lands when the
//!   seed-table-to-DB swap ships).
//! - Lookup by login slug (Redmine convention: `/users/:login`).
//! - Same schema-divergence caveat as W1/W2/W3: row goes in the
//!   undeclared lowercase `user` table, not the SCHEMAFULL
//!   PascalCase `ProjectActor`.

use surrealdb_types::{RecordId, SurrealValue};

use crate::{Store, StoreError};

/// Input for [`Store::create_user`].
#[derive(Debug, Clone, SurrealValue)]
pub struct NewUser {
    /// Lowercase login (URL slug). Redmine + OpenProject convention.
    pub login: String,
    /// Human-readable display name.
    pub display_name: String,
}

/// Row returned by [`Store::find_user_by_login`] / [`Store::list_users`].
#[derive(Debug, Clone, SurrealValue)]
pub struct UserRow {
    /// SurrealDB record id (`user:<ulid>`).
    pub id: Option<RecordId>,
    /// Lowercase login.
    pub login: String,
    /// Display name.
    pub display_name: String,
}

impl Store {
    /// Insert a User.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Surreal`] / [`StoreError::NotFound`] same as
    ///   the other resources.
    pub async fn create_user(&self, new: NewUser) -> Result<UserRow, StoreError> {
        let row: Option<UserRow> = self.db().create("user").content(new).await?;
        row.ok_or(StoreError::NotFound)
    }

    /// Find a User by login slug.
    ///
    /// # Errors
    ///
    /// - [`StoreError::NotFound`] when no row matches.
    /// - [`StoreError::Surreal`] on driver failures.
    pub async fn find_user_by_login(&self, login: &str) -> Result<UserRow, StoreError> {
        match self
            .db()
            .query("SELECT * FROM user WHERE login = $login LIMIT 1")
            .bind(("login", login.to_string()))
            .await
        {
            Ok(mut res) => match res.take::<Vec<UserRow>>(0) {
                Ok(rows) => rows.into_iter().next().ok_or(StoreError::NotFound),
                Err(e) if e.is_not_found() => Err(StoreError::NotFound),
                Err(e) => Err(StoreError::Surreal(e)),
            },
            Err(e) if e.is_not_found() => Err(StoreError::NotFound),
            Err(e) => Err(StoreError::Surreal(e)),
        }
    }

    /// List every User, in insertion order.
    pub async fn list_users(&self) -> Result<Vec<UserRow>, StoreError> {
        match self.db().select::<Vec<UserRow>>("user").await {
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
    async fn create_then_find_by_login() {
        let store = Store::open().await.unwrap();
        let inserted = store
            .create_user(NewUser {
                login: "jsmith".to_string(),
                display_name: "John Smith".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(inserted.login, "jsmith");
        let fetched = store.find_user_by_login("jsmith").await.unwrap();
        assert_eq!(fetched.login, "jsmith");
        assert_eq!(fetched.display_name, "John Smith");
    }

    #[tokio::test]
    async fn find_by_login_returns_not_found_for_unknown() {
        let store = Store::open().await.unwrap();
        let err = store.find_user_by_login("nope").await.unwrap_err();
        assert!(matches!(err, StoreError::NotFound), "got {err:?}");
    }

    #[tokio::test]
    async fn list_users_empty_then_populated() {
        let store = Store::open().await.unwrap();
        assert!(store.list_users().await.unwrap().is_empty());
        for (login, name) in [("admin", "Admin"), ("jsmith", "John Smith")] {
            store
                .create_user(NewUser {
                    login: login.to_string(),
                    display_name: name.to_string(),
                })
                .await
                .unwrap();
        }
        let rows = store.list_users().await.unwrap();
        assert_eq!(rows.len(), 2);
    }
}
