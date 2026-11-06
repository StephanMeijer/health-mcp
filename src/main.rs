use std::sync::Arc;

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use tokio::signal::unix::{SignalKind, signal};
use vitals_mcp::{auth, db::Db};

fn env(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("{key} must be set"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let vitals_mcp::Endpoints {
        resource,
        api_resource,
        host,
    } = vitals_mcp::endpoints(&env("PUBLIC_URL")?)?;

    if let Ok(url) = std::env::var("DATABASE_OWNER_URL") {
        let owner = PgPoolOptions::new().max_connections(1).connect(&url).await?;
        sqlx::migrate!().run(&owner).await.context("migrations")?;
        owner.close().await;
    }
    let db = Db {
        app: PgPoolOptions::new()
            .connect(&env("DATABASE_URL")?)
            .await
            .context("DATABASE_URL")?,
        reader: vitals_mcp::db::reader_pool_options()
            .connect(&env("DATABASE_READER_URL")?)
            .await
            .context("DATABASE_READER_URL")?,
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
    let app = vitals_mcp::app(db, host, verifier, api_verifier);

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
