use std::sync::Arc;

use anyhow::Context;
use axum::{Router, middleware, routing::get};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::never::NeverSessionManager,
};
use sqlx::postgres::PgPoolOptions;
use tokio::signal::unix::{SignalKind, signal};
use vitals_mcp::{auth, db::Db, tools::Vitals};

fn env(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("{key} must be set"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let public_url = reqwest::Url::parse(&env("PUBLIC_URL")?).context("PUBLIC_URL")?;
    // The MCP endpoint URL is both the RFC 9728 resource and the required token audience.
    let resource = public_url.join("mcp")?.to_string();
    let host = match public_url.port() {
        Some(p) => format!("{}:{p}", public_url.host_str().context("PUBLIC_URL has no host")?),
        None => public_url.host_str().context("PUBLIC_URL has no host")?.to_owned(),
    };

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
        reader: PgPoolOptions::new()
            .connect(&env("DATABASE_READER_URL")?)
            .await
            .context("DATABASE_READER_URL")?,
    };
    let verifier = Arc::new(
        auth::Verifier::discover(&env("OIDC_ISSUER")?, &resource)
            .await
            .context("OIDC discovery")?,
    );

    // Stateless: any replica can serve any request, no sticky sessions needed in K8s.
    let mcp = StreamableHttpService::new(
        move || Ok(Vitals::new(db.clone())),
        Arc::new(NeverSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_allowed_hosts([host]),
    );

    let app = Router::new()
        .nest_service("/mcp", mcp)
        .layer(middleware::from_fn_with_state(verifier.clone(), auth::require))
        .route("/.well-known/oauth-protected-resource", get(auth::metadata))
        .route("/.well-known/oauth-protected-resource/mcp", get(auth::metadata))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(verifier);

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
