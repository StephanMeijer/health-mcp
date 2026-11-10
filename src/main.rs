use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use health_mcp::{auth, db::Db};
use sqlx::postgres::PgPoolOptions;
use tokio::signal::unix::{SignalKind, signal};

/// Typed tools and ingest. A request that waits longer than `ACQUIRE_TIMEOUT` for a connection fails retryably:
/// a retry tool error or 503 from the typed tools, a 500 from ingest and devices.
const APP_CONNECTIONS: u32 = 10;
/// `sql_query` only: fewer, so untrusted SQL can't take every connection the typed tools need.
const READER_CONNECTIONS: u32 = 5;
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

fn env(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("{key} must be set"))
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

/// "3h", "90m", "45s" or plain seconds.
fn parse_duration(s: &str) -> anyhow::Result<Duration> {
    let (num, mult) = match s.chars().last() {
        Some('h') => (&s[..s.len() - 1], 3600),
        Some('m') => (&s[..s.len() - 1], 60),
        Some('s') => (&s[..s.len() - 1], 1),
        _ => (s, 1),
    };
    let n = num
        .trim()
        .parse::<u64>()
        .with_context(|| format!("bad duration {s:?}"))?;
    let secs = n
        .checked_mul(mult)
        .with_context(|| format!("duration {s:?} too large"))?;
    // Larger values break the `now - stale_after` timestamp arithmetic in close_stale.
    anyhow::ensure!(
        secs <= MAX_STALE_AFTER_SECS,
        "WORKOUT_STALE_AFTER must be at most 365 days, got {s:?}"
    );
    Ok(Duration::from_secs(secs))
}

const MAX_STALE_AFTER_SECS: u64 = 365 * 24 * 3600;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let health_mcp::Endpoints {
        resource,
        api_resource,
        host,
    } = health_mcp::endpoints(&env("PUBLIC_URL")?)?;

    if let Ok(url) = std::env::var("DATABASE_OWNER_URL") {
        let owner = PgPoolOptions::new().max_connections(1).connect(&url).await?;
        sqlx::migrate!().run(&owner).await.context("migrations")?;
        owner.close().await;
    }
    let db = Db {
        app: PgPoolOptions::new()
            .max_connections(APP_CONNECTIONS)
            .acquire_timeout(ACQUIRE_TIMEOUT)
            .connect(&env("DATABASE_URL")?)
            .await
            .context("DATABASE_URL")?,
        reader: health_mcp::db::reader_pool_options()
            .max_connections(READER_CONNECTIONS)
            .acquire_timeout(ACQUIRE_TIMEOUT)
            .connect(&env("DATABASE_READER_URL")?)
            .await
            .context("DATABASE_READER_URL")?,
        gym: health_mcp::db::GymConfig {
            default_timezone: env_or("DEFAULT_TIMEZONE", "Europe/Amsterdam"),
            stale_after: parse_duration(&env_or("WORKOUT_STALE_AFTER", "3h"))?,
        },
    };
    let issuer = env("OIDC_ISSUER")?;
    let verifier = Arc::new(
        auth::Verifier::discover(&issuer, &resource)
            .await
            .context("OIDC discovery")?,
    );
    let api_verifier = Arc::new(
        auth::Verifier::discover(&issuer, &api_resource)
            .await
            .context("OIDC discovery")?,
    );
    let app = health_mcp::app(db, host, verifier, api_verifier);

    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("listening on {bind}, resource {resource}");
    axum::serve(listener, app).with_graceful_shutdown(shutdown()).await?;
    Ok(())
}

/// Kubernetes and Docker stop containers with SIGTERM; PID 1 ignores it unless handled.
async fn shutdown() {
    let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
    tracing::info!("shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("3h").unwrap(), Duration::from_secs(3 * 3600));
        assert_eq!(parse_duration("90m").unwrap(), Duration::from_secs(90 * 60));
        assert_eq!(parse_duration("45s").unwrap(), Duration::from_secs(45));
        assert_eq!(parse_duration("10").unwrap(), Duration::from_secs(10));
        for bad in ["", "h", "-1h", "3d", "1.5h"] {
            assert!(parse_duration(bad).is_err(), "{bad:?}");
        }
        let err = parse_duration("5124095576030432h").unwrap_err().to_string();
        assert!(err.contains("too large"), "{err}");
        assert_eq!(parse_duration("8760h").unwrap(), Duration::from_secs(365 * 24 * 3600));
        for bad in ["8761h", "31536001", "9300000000000000"] {
            let err = parse_duration(bad).unwrap_err().to_string();
            assert!(err.contains("at most 365 days"), "{bad:?}: {err}");
        }
    }
}
