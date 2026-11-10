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
    /// `vitals_app`: typed tools and ingest on `vitals_data.*`; no RLS, every query filters on `user_id` itself.
    pub app: PgPool,
    /// `vitals_reader`: the SQL tool, read-only views in schema `vitals`.
    pub reader: PgPool,
    pub gym: GymConfig,
}

/// Settings of the gym tools (`DEFAULT_TIMEZONE`, `WORKOUT_STALE_AFTER`).
#[derive(Clone, Debug)]
pub struct GymConfig {
    /// Timezone of users who haven't set one.
    pub default_timezone: String,
    /// A workout with no activity for this long is closed by the next workout action.
    pub stale_after: std::time::Duration,
}

impl Default for GymConfig {
    fn default() -> Self {
        Self {
            default_timezone: "Europe/Amsterdam".into(),
            stale_after: std::time::Duration::from_secs(3 * 3600),
        }
    }
}

impl Db {
    /// The gym tools' view of this database: the app pool (role `vitals_app`) plus their settings.
    pub fn gym_app(&self) -> crate::gym::db::App {
        crate::gym::db::App {
            pool: self.app.clone(),
            default_timezone: self.gym.default_timezone.clone(),
            stale_after: self.gym.stale_after,
        }
    }

    /// Readiness: both pools reach the database within 2 s. Checks reachability, not free capacity: an idle
    /// connection runs `SELECT 1`; when every connection is busy, those open connections count as reachable; only an
    /// empty pool connects.
    pub async fn ready(&self) -> bool {
        async fn ping(pool: &PgPool) -> sqlx::Result<()> {
            match pool.try_acquire() {
                Some(mut conn) => sqlx::query("SELECT 1").execute(&mut *conn).await.map(drop),
                None if pool.size() > 0 => Ok(()),
                None => sqlx::query("SELECT 1").execute(pool).await.map(drop),
            }
        }
        let ping = async {
            ping(&self.app).await?;
            ping(&self.reader).await
        };
        matches!(
            tokio::time::timeout(std::time::Duration::from_secs(2), ping).await,
            Ok(Ok(()))
        )
    }

    /// Transaction for the typed tools and ingest. Queries must filter on `user` themselves: nothing in the database
    /// checks it (no RLS), so the signed user settings `reader_tx` needs aren't set here.
    pub async fn app_tx(&self) -> sqlx::Result<Tx> {
        let mut tx = self.app.begin().await?;
        sqlx::query("SET LOCAL statement_timeout = '30s'")
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
