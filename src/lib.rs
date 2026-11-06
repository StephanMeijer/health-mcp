pub mod auth;
pub mod db;
pub mod ingest;
pub mod tools;

use std::sync::Arc;

use anyhow::Context;
use axum::{Router, middleware, routing::get};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::never::NeverSessionManager,
};

/// What `PUBLIC_URL` determines: the MCP and API resources (each the RFC 9728 resource and the required
/// token audience, so MCP tokens don't work on `/api` and vice versa) and the only allowed `Host` for `/mcp`.
#[derive(Debug)]
pub struct Endpoints {
    pub resource: String,
    pub api_resource: String,
    pub host: String,
}

pub fn endpoints(public_url: &str) -> anyhow::Result<Endpoints> {
    let public_url = reqwest::Url::parse(public_url).context("PUBLIC_URL")?;
    anyhow::ensure!(
        public_url.path() == "/",
        "PUBLIC_URL must be an origin without a path, e.g. https://vitals.example.com"
    );
    let host_str = public_url.host_str().context("PUBLIC_URL has no host")?;
    Ok(Endpoints {
        resource: public_url.join("mcp")?.to_string(),
        api_resource: public_url.join("api")?.to_string(),
        host: match public_url.port() {
            Some(p) => format!("{host_str}:{p}"),
            None => host_str.to_owned(),
        },
    })
}

/// All routes. `verifier` guards `/mcp`, `api_verifier` guards `/api`: each has its own audience,
/// so MCP tokens don't work on `/api` and vice versa. `host` is the only allowed `Host` for `/mcp`.
pub fn app(db: db::Db, host: String, verifier: Arc<auth::Verifier>, api_verifier: Arc<auth::Verifier>) -> Router {
    let api = ingest::router(db.clone()).layer(middleware::from_fn_with_state(api_verifier.clone(), auth::require));

    // Stateless: any replica can serve any request, no sticky sessions needed in K8s.
    let mcp = StreamableHttpService::new(
        move || Ok(tools::Vitals::new(db.clone())),
        Arc::new(NeverSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_allowed_hosts([host]),
    );

    Router::new()
        .nest_service("/mcp", mcp)
        .layer(middleware::from_fn_with_state(verifier.clone(), auth::require))
        // After the MCP auth layer, which only wraps routes added before it.
        .nest("/api", api)
        .route("/.well-known/oauth-protected-resource", get(auth::metadata))
        .route("/.well-known/oauth-protected-resource/mcp", get(auth::metadata))
        .route(
            "/.well-known/oauth-protected-resource/api",
            get(auth::metadata).with_state(api_verifier),
        )
        .route("/healthz", get(|| async { "ok" }))
        .with_state(verifier)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use jsonwebtoken::{EncodingKey, Header, encode, jwk::Jwk, jwk::JwkSet};
    use tower::ServiceExt;

    use super::*;

    const SECRET: &str = "test-secret-at-least-32-bytes-long!!";
    const ISS: &str = "https://auth.example.com";
    const MCP: &str = "https://vitals.example.com/mcp";
    const API: &str = "https://vitals.example.com/api";

    fn verifier(aud: &str) -> Arc<auth::Verifier> {
        let mut k = Jwk::from_encoding_key(
            &EncodingKey::from_secret(SECRET.as_bytes()),
            jsonwebtoken::Algorithm::HS256,
        )
        .unwrap();
        k.common.key_id = Some("k1".into());
        let jwks = JwkSet { keys: vec![k] };
        Arc::new(auth::Verifier::new(
            ISS,
            aud,
            "http://127.0.0.1:9/jwks".into(),
            jwks,
            reqwest::Client::new(),
        ))
    }

    fn bearer(aud: &str) -> String {
        let mut h = Header::new(jsonwebtoken::Algorithm::HS256);
        h.typ = Some("at+jwt".into());
        h.kid = Some("k1".into());
        let now = chrono::Utc::now().timestamp();
        let claims = serde_json::json!({"iss": ISS, "aud": aud, "sub": "alice", "exp": now + 300});
        format!(
            "Bearer {}",
            encode(&h, &claims, &EncodingKey::from_secret(SECRET.as_bytes())).unwrap()
        )
    }

    async fn call(path: &str, token: Option<&str>) -> (StatusCode, Option<String>) {
        // Lazy pools: no request below reaches the database.
        let pool = || sqlx::PgPool::connect_lazy("postgres://localhost/none").unwrap();
        let db = db::Db {
            app: pool(),
            reader: pool(),
        };
        let app = app(db, "vitals.example.com".into(), verifier(MCP), verifier(API));
        // An unsupported Content-Type: past auth, /api answers 415 without touching the database.
        let mut req = Request::post(path).header(header::CONTENT_TYPE, "text/plain");
        if let Some(aud) = token {
            req = req.header(header::AUTHORIZATION, bearer(aud));
        }
        let res = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        let challenge = res
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .map(|h| h.to_str().unwrap().to_owned());
        (res.status(), challenge)
    }

    const API_CHALLENGE: &str =
        r#"Bearer resource_metadata="https://vitals.example.com/.well-known/oauth-protected-resource/api""#;
    const MCP_CHALLENGE: &str =
        r#"Bearer resource_metadata="https://vitals.example.com/.well-known/oauth-protected-resource/mcp""#;

    #[test]
    fn endpoints_come_from_public_url() {
        let e = endpoints("https://vitals.example.com").unwrap();
        assert_eq!(
            (e.resource.as_str(), e.api_resource.as_str(), e.host.as_str()),
            (MCP, API, "vitals.example.com")
        );
        let e = endpoints("https://vitals.example.com:8443/").unwrap();
        assert_eq!(
            (e.resource.as_str(), e.host.as_str()),
            ("https://vitals.example.com:8443/mcp", "vitals.example.com:8443")
        );
        let err = endpoints("https://vitals.example.com/x").unwrap_err().to_string();
        assert!(err.contains("PUBLIC_URL"), "{err}");
    }

    #[tokio::test]
    async fn each_endpoint_accepts_only_its_own_audience() {
        let invalid = |c: &str| Some(format!(r#"{c}, error="invalid_token""#));
        assert_eq!(
            call("/api/v1/ingest", None).await,
            (StatusCode::UNAUTHORIZED, Some(API_CHALLENGE.into()))
        );
        assert_eq!(
            call("/api/v1/ingest", Some(MCP)).await,
            (StatusCode::UNAUTHORIZED, invalid(API_CHALLENGE))
        );
        assert_eq!(
            call("/api/v1/tools/summary", Some(MCP)).await,
            (StatusCode::UNAUTHORIZED, invalid(API_CHALLENGE))
        );
        assert_eq!(
            call("/mcp", Some(API)).await,
            (StatusCode::UNAUTHORIZED, invalid(MCP_CHALLENGE))
        );
        assert_eq!(
            call("/api/v1/ingest", Some(API)).await,
            (StatusCode::UNSUPPORTED_MEDIA_TYPE, None)
        );
    }
}
