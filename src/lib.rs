pub mod auth;
pub mod db;
pub mod gym;
pub mod ingest;
pub mod tools;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::http::StatusCode;
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
        "PUBLIC_URL must be an origin without a path, e.g. https://health.example.com"
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

/// Requests `/mcp` and `/api` run at once; more wait their turn. A waiting request hasn't read its body yet, so this
/// also bounds the memory ingest bodies (up to 32 MiB each) can take.
pub const MAX_CONCURRENT_REQUESTS: usize = 16;
/// Above the 30 s statement timeout, so a slow query fails with its own error first. Includes time spent waiting.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// How long `/readyz` reuses its last database check.
const READY_CACHE: Duration = Duration::from_secs(1);

/// Bounds the work of every route added to `router` so far (not its fallback): `max` requests at once across all of
/// them (one shared semaphore), each answered `408` after `timeout`. The slot is taken inside the timed future, so
/// time spent waiting for one counts towards `timeout`.
fn limits<S: Clone + Send + Sync + 'static>(router: Router<S>, max: usize, timeout: Duration) -> Router<S> {
    let slots = Arc::new(tokio::sync::Semaphore::new(max));
    router
        .route_layer(middleware::from_fn(move |req, next: middleware::Next| {
            let slots = slots.clone();
            async move {
                let _slot = slots.acquire_owned().await.expect("never closed");
                next.run(req).await
            }
        }))
        .route_layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            timeout,
        ))
}

/// All routes. `verifier` guards `/mcp`, `api_verifier` guards `/api`: each has its own audience,
/// so MCP tokens don't work on `/api` and vice versa. `host` is the only allowed `Host` for `/mcp`.
pub fn app(db: db::Db, host: String, verifier: Arc<auth::Verifier>, api_verifier: Arc<auth::Verifier>) -> Router {
    let ready_db = db.clone();
    let last_ready = Arc::new(tokio::sync::Mutex::new(None::<(std::time::Instant, bool)>));
    let api = ingest::router(db.clone()).layer(middleware::from_fn_with_state(api_verifier.clone(), auth::require));

    // Stateless: any replica can serve any request, no sticky sessions needed in K8s.
    let mcp = StreamableHttpService::new(
        move || Ok(tools::Health::new(db.clone())),
        Arc::new(NeverSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_allowed_hosts([host]),
    );

    let routes = Router::new()
        .nest_service("/mcp", mcp)
        .layer(middleware::from_fn_with_state(verifier.clone(), auth::require))
        // After the MCP auth layer, which only wraps routes added before it.
        .nest("/api", api);
    // Only /mcp and /api: discovery and health checks never queue behind them.
    limits(routes, MAX_CONCURRENT_REQUESTS, REQUEST_TIMEOUT)
        .route("/.well-known/oauth-protected-resource", get(auth::metadata))
        .route("/.well-known/oauth-protected-resource/mcp", get(auth::metadata))
        .route(
            "/.well-known/oauth-protected-resource/api",
            get(auth::metadata).with_state(api_verifier),
        )
        // Outermost, so rejected requests (401, 408, 413) are logged too. Method and path only: query strings and
        // headers (the bearer token) never reach the log. Health checks are added after it to keep probes out.
        .layer(
            tower_http::trace::TraceLayer::new_for_http()
                .make_span_with(|req: &axum::extract::Request| {
                    tracing::info_span!("request", method = %req.method(), path = req.uri().path())
                })
                .on_response(tower_http::trace::DefaultOnResponse::new().level(tracing::Level::INFO)),
        )
        // Liveness: the process answers. Readiness: it can also reach the database.
        .route("/healthz", get(|| async { "ok" }))
        .route(
            "/readyz",
            get(move || {
                let (db, last) = (ready_db.clone(), last_ready.clone());
                async move {
                    // Unauthenticated: one check at a time per process, and concurrent probes share its result.
                    let mut last = last.lock().await;
                    let ready = match *last {
                        Some((at, ready)) if at.elapsed() < READY_CACHE => ready,
                        _ => {
                            let ready = db.ready().await;
                            *last = Some((std::time::Instant::now(), ready));
                            ready
                        }
                    };
                    match ready {
                        true => (StatusCode::OK, "ok"),
                        false => (StatusCode::SERVICE_UNAVAILABLE, "database unavailable"),
                    }
                }
            }),
        )
        .with_state(verifier)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, header};
    use jsonwebtoken::{EncodingKey, Header, encode, jwk::Jwk, jwk::JwkSet};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn limits_time_out_and_bound_concurrency() {
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
        let (running, peak) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let slow = {
            let (running, peak) = (running.clone(), peak.clone());
            get(move || async move {
                peak.fetch_max(running.fetch_add(1, SeqCst) + 1, SeqCst);
                tokio::time::sleep(Duration::from_millis(100)).await;
                running.fetch_sub(1, SeqCst);
            })
        };
        let get_slow = |app: Router| async move {
            let res = app
                .oneshot(Request::get("/slow").body(Body::empty()).unwrap())
                .await
                .unwrap();
            res.status()
        };

        let one_at_a_time = limits(Router::new().route("/slow", slow.clone()), 1, Duration::from_secs(5));
        let statuses = tokio::join!(get_slow(one_at_a_time.clone()), get_slow(one_at_a_time));
        assert_eq!(statuses, (StatusCode::OK, StatusCode::OK));
        assert_eq!(peak.load(SeqCst), 1, "the second request ran while the first did");

        let impatient = limits(Router::new().route("/slow", slow.clone()), 1, Duration::from_millis(10));
        assert_eq!(get_slow(impatient).await, StatusCode::REQUEST_TIMEOUT);

        // 100 ms waiting for the slot plus 100 ms running: the wait counts towards the timeout.
        let queued = limits(Router::new().route("/slow", slow), 1, Duration::from_millis(150));
        let statuses = tokio::join!(get_slow(queued.clone()), get_slow(queued));
        assert_eq!(statuses, (StatusCode::OK, StatusCode::REQUEST_TIMEOUT));
    }

    #[tokio::test]
    async fn limits_cover_nested_routes_but_not_the_fallback() {
        let slow = || Router::new().route("/slow", get(|| tokio::time::sleep(Duration::from_millis(100))));
        let app = limits(
            Router::new().nest_service("/mcp", slow()).nest("/api", slow()),
            1,
            Duration::from_millis(150),
        );
        let status = |path: &'static str| {
            let app = app.clone();
            async move {
                app.oneshot(Request::get(path).body(Body::empty()).unwrap())
                    .await
                    .unwrap()
                    .status()
            }
        };
        // Both nested routes share the one slot, so the second waits and times out; the fallback never waits.
        let statuses = tokio::join!(status("/mcp/slow"), status("/api/slow"), async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            tokio::time::timeout(Duration::from_millis(50), status("/unknown")).await
        });
        assert_eq!(
            statuses,
            (StatusCode::OK, StatusCode::REQUEST_TIMEOUT, Ok(StatusCode::NOT_FOUND))
        );
    }

    #[tokio::test]
    async fn readyz_reports_an_unreachable_database() {
        // Port 1: nothing listens, so connecting fails at once.
        let pool = || sqlx::PgPool::connect_lazy("postgres://localhost:1/none").unwrap();
        let db = db::Db {
            app: pool(),
            reader: pool(),
            gym: Default::default(),
        };
        let app = app(db, "health.example.com".into(), verifier(MCP), verifier(API));
        let res = app
            .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    const SECRET: &str = "test-secret-at-least-32-bytes-long!!";
    const ISS: &str = "https://auth.example.com";
    const MCP: &str = "https://health.example.com/mcp";
    const API: &str = "https://health.example.com/api";

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
            gym: Default::default(),
        };
        let app = app(db, "health.example.com".into(), verifier(MCP), verifier(API));
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
        r#"Bearer resource_metadata="https://health.example.com/.well-known/oauth-protected-resource/api""#;
    const MCP_CHALLENGE: &str =
        r#"Bearer resource_metadata="https://health.example.com/.well-known/oauth-protected-resource/mcp""#;

    /// A JSON-RPC request to `/mcp` with a valid MCP token; returns the response body.
    async fn mcp(body: serde_json::Value) -> String {
        // Lazy pools that give up at once: listing and routing tools needs no database.
        let pool = || {
            sqlx::postgres::PgPoolOptions::new()
                .acquire_timeout(Duration::from_millis(200))
                .connect_lazy("postgres://localhost:1/none")
                .unwrap()
        };
        let db = db::Db {
            app: pool(),
            reader: pool(),
            gym: Default::default(),
        };
        let app = app(db, "health.example.com".into(), verifier(MCP), verifier(API));
        let req = Request::post("/mcp")
            .header(header::HOST, "health.example.com")
            .header(header::AUTHORIZATION, bearer(MCP))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .body(Body::from(body.to_string()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
        String::from_utf8(body.to_vec()).unwrap()
    }

    /// #115: the MCP handler must serve the gym tools too, not only the vitals router `#[tool_handler]` defaults to.
    #[tokio::test]
    async fn mcp_lists_and_routes_vitals_and_gym_tools() {
        let list = mcp(serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})).await;
        let list: serde_json::Value = serde_json::from_str(&list).unwrap();
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("{list}"))
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        for name in [
            "reading_list",
            "sql_query",
            "workout_list",
            "set_add",
            "training_overview",
        ] {
            assert!(names.contains(&name), "{name} missing from tools/list: {names:?}");
        }
        assert_eq!(names.len(), tools::Health::tool_list().len());

        // Routed to the gym tool: it then fails on the lazy database, but it is found.
        let call = mcp(serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "workout_list", "arguments": {}}}))
        .await;
        assert!(!call.contains("-32602") && !call.contains("not found"), "{call}");
        assert!(call.contains(r#""result""#), "{call}");
    }

    #[test]
    fn endpoints_come_from_public_url() {
        let e = endpoints("https://health.example.com").unwrap();
        assert_eq!(
            (e.resource.as_str(), e.api_resource.as_str(), e.host.as_str()),
            (MCP, API, "health.example.com")
        );
        let e = endpoints("https://health.example.com:8443/").unwrap();
        assert_eq!(
            (e.resource.as_str(), e.host.as_str()),
            ("https://health.example.com:8443/mcp", "health.example.com:8443")
        );
        let err = endpoints("https://health.example.com/x").unwrap_err().to_string();
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
