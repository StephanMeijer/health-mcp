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
    pub share: ShareConfig,
}

/// Settings of sharing: where invite links point and how invite mails go out.
#[derive(Clone)]
pub struct ShareConfig {
    /// `PUBLIC_URL`, with a trailing slash; invite links are `{public_url}invite/{code}`.
    pub public_url: String,
    /// `None`: no mail is sent, and `share_invite` returns the link to pass on by hand.
    pub mailer: Option<std::sync::Arc<crate::mail::Mailer>>,
    /// `None`: the invite page can't log in, and says to accept in the app or Claude.
    pub login: Option<std::sync::Arc<crate::web::WebLogin>>,
}

impl Default for ShareConfig {
    fn default() -> Self {
        Self {
            public_url: "http://localhost:8080/".into(),
            mailer: None,
            login: None,
        }
    }
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

    /// Readiness: both pools reach the database within 2 s. Checks reachability, not free capacity: when every
    /// connection is busy, those open connections count as reachable; otherwise the pool runs `SELECT 1`. The pool
    /// tests an idle connection before handing it out and replaces a dead one (sqlx's `test_before_acquire`, on by
    /// default), so dead connections left by a database restart don't keep this false.
    pub async fn ready(&self) -> bool {
        async fn ping(pool: &PgPool) -> sqlx::Result<()> {
            if pool.size() > 0 && pool.num_idle() == 0 {
                return Ok(());
            }
            sqlx::query("SELECT 1").execute(pool).await.map(drop)
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

    /// Read-only transaction for untrusted SQL over `user`'s data in `domains`. The reader role cannot sign, so the
    /// signature comes from the app pool. It covers the domains too (migration 0011).
    pub async fn reader_tx(&self, user: &str, domains: crate::share::Domains) -> sqlx::Result<Tx> {
        let domains = domains.to_string();
        let sig: String = sqlx::query_scalar("SELECT vitals_private.sign($1 || '/' || $2)")
            .bind(&domains)
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
        sqlx::query(
            "SELECT set_config('vitals.user_id', $1, true), set_config('vitals.user_sig', $2, true),
                    set_config('vitals.domains', $3, true)",
        )
        .bind(user)
        .bind(&sig)
        .bind(&domains)
        .execute(&mut *tx)
        .await?;
        Ok(tx)
    }
}

/// What a database error means for whoever made the request. The typed tools, ingest and the device endpoints answer
/// with it; `sql_query` doesn't (there every message is about the caller's own SQL).
#[derive(Debug, PartialEq)]
pub enum DbFault {
    /// The data was refused for good (SQLSTATE class 22 data exception, class 23 integrity constraint violation,
    /// including CHECKs and the reading range trigger): the message says what to change.
    Caller(String),
    /// Nothing ran and trying again is safe: see [`retryable`], or every pooled connection was busy.
    Retry,
    /// Anything else is the server's own fault (a missing grant, an undefined column, a statement timeout): log it,
    /// tell the caller nothing more.
    Server,
}

pub fn classify(e: &sqlx::Error) -> DbFault {
    match e {
        sqlx::Error::Database(d) => match d.code() {
            Some(c) if retryable(&c) => DbFault::Retry,
            Some(c) if c.starts_with("22") || c.starts_with("23") => DbFault::Caller(d.message().to_owned()),
            _ => DbFault::Server,
        },
        sqlx::Error::PoolTimedOut => DbFault::Retry,
        _ => DbFault::Server,
    }
}

/// SQLSTATE class 40 (transaction rollback: 40001 serialization, 40P01 deadlock) except 40002, a constraint
/// violation that a retry repeats; connection lost (08000, 08003, 08006); too many connections (53300) or disk full
/// (53100); and 57P01–57P03 (server shutting down or starting up, as in a restart or failover). Not the rest of 08
/// and 53: a protocol violation (08P01), out of memory (53200) or a temp file limit (53400) fails the same way again.
/// 57014 (a statement timeout) is not: the same query times out again.
pub fn retryable(code: &str) -> bool {
    (code.starts_with("40") && code != "40002")
        || matches!(
            code,
            "08000" | "08003" | "08006" | "53300" | "53100" | "57P01" | "57P02" | "57P03"
        )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn transient_errors_are_retryable() {
        for code in ["40001", "40P01", "08006", "53300", "53100", "57P01", "57P03"] {
            assert!(retryable(code), "{code}");
        }
        for code in ["40002", "57014", "23514", "42601", "22P02", "08P01", "53200", "53400"] {
            assert!(!retryable(code), "{code}");
        }
    }

    /// A database error with SQLSTATE `code`, as Postgres would return it (message "message").
    pub(crate) fn pg(code: &str) -> sqlx::Error {
        #[derive(Debug)]
        struct E(String);
        impl std::fmt::Display for E {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "error {}", self.0)
            }
        }
        impl std::error::Error for E {}
        impl sqlx::error::DatabaseError for E {
            fn message(&self) -> &str {
                "message"
            }
            fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
                Some(self.0.as_str().into())
            }
            fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
                self
            }
            fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
                self
            }
            fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
                self
            }
            fn kind(&self) -> sqlx::error::ErrorKind {
                sqlx::error::ErrorKind::Other
            }
        }
        sqlx::Error::Database(Box::new(E(code.into())))
    }

    #[test]
    fn classifies_by_sqlstate() {
        for code in ["23514", "23505", "23503", "22P02", "22007"] {
            assert_eq!(classify(&pg(code)), DbFault::Caller("message".into()), "{code}");
        }
        for code in ["40P01", "40001", "57P01", "08006"] {
            assert_eq!(classify(&pg(code)), DbFault::Retry, "{code}");
        }
        for code in ["42501", "42703", "57014", "XX000", "40002"] {
            assert_eq!(classify(&pg(code)), DbFault::Server, "{code}");
        }
        assert_eq!(classify(&sqlx::Error::PoolTimedOut), DbFault::Retry);
        assert_eq!(classify(&sqlx::Error::RowNotFound), DbFault::Server);
    }
}
