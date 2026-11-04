use sqlx::{PgPool, Postgres, Transaction};

pub type Tx = Transaction<'static, Postgres>;

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
                    set_config('vitals.user_sig', vitals_private.sign($1), true)",
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
