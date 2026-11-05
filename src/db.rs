use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, Transaction};

pub type Tx = Transaction<'static, Postgres>;

/// Reader pool options with a release hook that scrubs the session state a `sql_query` caller can leave behind.
/// A read-only `SELECT` can still take a session-level advisory lock or set a session GUC, and neither is cleared
/// by the transaction `ROLLBACK` sql_query issues, so without this a lock could ride a pooled connection into the
/// next caller — or wedge the startup migration's own advisory lock. `pg_advisory_unlock_all()` + `RESET ALL`
/// clear both without deallocating prepared statements (`DISCARD ALL` would, breaking sqlx's statement cache).
/// The app pool needs none of this: it only runs server-authored SQL.
pub fn reader_pool_options() -> PgPoolOptions {
    PgPoolOptions::new().after_release(|conn, _meta| {
        Box::pin(async move {
            sqlx::query("SELECT pg_advisory_unlock_all()")
                .execute(&mut *conn)
                .await?;
            sqlx::query("RESET ALL").execute(&mut *conn).await?;
            Ok(true)
        })
    })
}

#[derive(Clone)]
pub struct Db {
    /// `vitals_app`: typed tools, RLS on `vitals_data.*`.
    pub app: PgPool,
    /// `vitals_reader`: the SQL tool, read-only views in schema `vitals`.
    pub reader: PgPool,
}

impl Db {
    /// Transaction for the typed tools; RLS limits it to `user`'s rows (writes: manual rows only).
    pub async fn app_tx(&self, user: &str) -> sqlx::Result<Tx> {
        let mut tx = self.app.begin().await?;
        sqlx::query(
            "SELECT set_config('vitals.user_id', $1, true),
                    set_config('vitals.user_sig', vitals_private.sign($1), true),
                    set_config('statement_timeout', '30s', true)",
        )
        .bind(user)
        .execute(&mut *tx)
        .await?;
        Ok(tx)
    }

    /// Read-only transaction for untrusted SQL. The reader role cannot sign, so the signature comes from the app pool.
    pub async fn reader_tx(&self, user: &str) -> sqlx::Result<Tx> {
        let sig: String = sqlx::query_scalar("SELECT vitals_private.sign($1)")
            .bind(user)
            .fetch_one(&self.app)
            .await?;
        let mut tx = self.reader.begin().await?;
        sqlx::query("SET TRANSACTION READ ONLY").execute(&mut *tx).await?;
        sqlx::query("SET LOCAL statement_timeout = '5s'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SET LOCAL search_path = vitals, public")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT set_config('vitals.user_id', $1, true), set_config('vitals.user_sig', $2, true)")
            .bind(user)
            .bind(&sig)
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }
}
