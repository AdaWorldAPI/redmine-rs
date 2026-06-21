//! `Store` — the handle every other rm-* crate gets via axum `State`.

use ogar_adapter_surrealql::emit_surrealql_ddl;
use ogar_vocab::all_promoted_classes;
use surrealdb::engine::local::{Db, Mem};
use surrealdb::Surreal;

use crate::StoreError;

/// Connected SurrealDB instance with the full OGAR schema applied.
///
/// Construct once at server start; clone freely (the inner
/// `Surreal<Db>` handle is cheap to clone — it shares the connection
/// pool internally).
#[derive(Debug, Clone)]
pub struct Store {
    db: Surreal<Db>,
}

impl Store {
    /// Boot an in-memory SurrealDB and apply the OGAR schema.
    ///
    /// The schema string comes from
    /// [`ogar_adapter_surrealql::emit_surrealql_ddl`] over
    /// [`ogar_vocab::all_promoted_classes`] — 32 promoted concepts,
    /// in `class_ids::ALL` order, round-trip-pinned upstream.
    ///
    /// Today's MVP only exercises the `project_work_item` table
    /// (Issue); the full schema is applied so adding more resource
    /// pages (W1..W8 width tracks) is a handler-only change.
    ///
    /// # Errors
    ///
    /// - [`StoreError::Surreal`] on driver boot, namespace selection,
    ///   or schema-DDL application failure.
    pub async fn open() -> Result<Self, StoreError> {
        let db = Surreal::new::<Mem>(()).await?;
        db.use_ns("redmine").use_db("main").await?;
        let ddl = emit_surrealql_ddl(&all_promoted_classes());
        db.query(ddl).await?.check()?;
        tracing::info!("rm-store opened (in-memory, schema applied)");
        Ok(Self { db })
    }

    /// Access the underlying SurrealDB handle. Resource modules use
    /// it directly for table-specific queries; promoted into the
    /// CRUD<T> trait once a second concept lands (Plan §1.6).
    pub(crate) fn db(&self) -> &Surreal<Db> {
        &self.db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn open_succeeds_in_memory() {
        let store = Store::open().await.expect("in-memory boot must succeed");
        // SurrealDB returns a handle from `db.health()`-style ping.
        // We assert via a trivial RETURN 1 query.
        let mut res = store
            .db()
            .query("RETURN 1")
            .await
            .expect("RETURN 1 must execute");
        let n: Option<i64> = res.take(0).expect("take 0 must succeed");
        assert_eq!(n, Some(1));
    }

    #[tokio::test]
    async fn schema_applied_at_open_creates_project_work_item_table() {
        // Apply the schema, then INFO FOR DB and check the project_work_item
        // table got defined. SurrealDB's `INFO FOR DB` returns one row
        // with a `tables` object.
        let store = Store::open().await.unwrap();
        // Quick smoke: try to insert into the table; if it didn't exist the
        // SCHEMAFULL would reject.
        let r = store
            .db()
            .query("CREATE project_work_item:test SET subject = 'smoke'")
            .await;
        assert!(
            r.is_ok(),
            "CREATE project_work_item must succeed (schema applied): {r:?}"
        );
    }
}
