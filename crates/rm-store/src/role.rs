//! Role (`project_role` codebook id `0x0117`) CRUD — the RBAC role
//! lookup. Both ports ship `Role` as the model name.
//!
//! W4 of the Redmine Integration Plan, second concept in the
//! actors-and-access track. Pair with [`crate::user`] for /users and
//! /roles top-level pages; nested `/projects/:id/members` (the
//! User × Project × Role join via `project_membership` +
//! `project_member_role`) is a W4 follow-up.

use surrealdb_types::{RecordId, SurrealValue};

use crate::{Store, StoreError};

/// Input for [`Store::create_role`].
#[derive(Debug, Clone, SurrealValue)]
pub struct NewRole {
    /// Role name (`"Developer"`, `"Reporter"`, `"Manager"`, …).
    pub name: String,
    /// Sort position (1-based). Redmine + OP both surface this.
    pub position: i64,
}

/// Row returned by [`Store::find_role_by_name`] / [`Store::list_roles`].
#[derive(Debug, Clone, SurrealValue)]
pub struct RoleRow {
    /// SurrealDB record id (`role:<ulid>`).
    pub id: Option<RecordId>,
    /// Role name.
    pub name: String,
    /// Sort position.
    pub position: i64,
}

impl Store {
    /// Insert a Role.
    pub async fn create_role(&self, new: NewRole) -> Result<RoleRow, StoreError> {
        let row: Option<RoleRow> = self.db().create("role").content(new).await?;
        row.ok_or(StoreError::NotFound)
    }

    /// Find a Role by its name (used by the URL slug; Redmine's
    /// `/roles/:id` uses numeric ids but our MVP keys on name).
    pub async fn find_role_by_name(&self, name: &str) -> Result<RoleRow, StoreError> {
        match self
            .db()
            .query("SELECT * FROM role WHERE name = $n LIMIT 1")
            .bind(("n", name.to_string()))
            .await
        {
            Ok(mut res) => match res.take::<Vec<RoleRow>>(0) {
                Ok(rows) => rows.into_iter().next().ok_or(StoreError::NotFound),
                Err(e) if e.is_not_found() => Err(StoreError::NotFound),
                Err(e) => Err(StoreError::Surreal(e)),
            },
            Err(e) if e.is_not_found() => Err(StoreError::NotFound),
            Err(e) => Err(StoreError::Surreal(e)),
        }
    }

    /// List every Role.
    pub async fn list_roles(&self) -> Result<Vec<RoleRow>, StoreError> {
        match self.db().select::<Vec<RoleRow>>("role").await {
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
            .create_role(NewRole {
                name: "Developer".to_string(),
                position: 1,
            })
            .await
            .unwrap();
        let r = store.find_role_by_name("Developer").await.unwrap();
        assert_eq!(r.name, "Developer");
        assert_eq!(r.position, 1);
    }

    #[tokio::test]
    async fn find_by_name_returns_not_found_for_unknown() {
        let store = Store::open().await.unwrap();
        let err = store.find_role_by_name("Nope").await.unwrap_err();
        assert!(matches!(err, StoreError::NotFound), "got {err:?}");
    }

    #[tokio::test]
    async fn list_roles_empty_then_populated() {
        let store = Store::open().await.unwrap();
        assert!(store.list_roles().await.unwrap().is_empty());
        for (i, n) in ["Manager", "Developer", "Reporter"].iter().enumerate() {
            store
                .create_role(NewRole {
                    name: n.to_string(),
                    position: (i as i64) + 1,
                })
                .await
                .unwrap();
        }
        let rows = store.list_roles().await.unwrap();
        assert_eq!(rows.len(), 3);
    }
}
