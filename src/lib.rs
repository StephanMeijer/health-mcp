pub mod auth;
pub mod db;
pub mod gym;
pub mod ingest;
pub mod mail;
pub mod session;
pub mod share;
pub mod tools;
pub mod web;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::http::{StatusCode, header};
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
/// so MCP tokens don't work on `/api` and vice versa. `host` is the only allowed `Host` for `/mcp`. `web` is the
/// web app ([`web_app`]).
pub fn app(
    db: db::Db,
    host: String,
    verifier: Arc<auth::Verifier>,
    api_verifier: Arc<auth::Verifier>,
    web: Router,
) -> Router {
    let ready_db = db.clone();
    let last_ready = Arc::new(tokio::sync::Mutex::new(None::<(std::time::Instant, bool)>));
    let sessions = session::Sessions::new(db.clone(), api_verifier.clone());
    // A bearer token (the phone apps) or the web app's session cookie.
    let api = ingest::router(db.clone()).layer(middleware::from_fn_with_state(sessions.clone(), session::api_auth));
    let invite = web::router(db.clone(), api_verifier.clone());

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
        .nest("/api", api)
        // Public: the invite code is the secret, and accepting logs in itself.
        .merge(invite)
        // The web app's login, logout and /app/me; the files themselves come from `web_app`.
        .merge(session::router(sessions));
    // Only /mcp and /api: discovery and health checks never queue behind them.
    limits(routes, MAX_CONCURRENT_REQUESTS, REQUEST_TIMEOUT)
        .route("/.well-known/oauth-protected-resource", get(auth::metadata))
        .route("/.well-known/oauth-protected-resource/mcp", get(auth::metadata))
        .route(
            "/.well-known/oauth-protected-resource/api",
            get(auth::metadata).with_state(api_verifier),
        )
        .merge(web.with_state(()))
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

/// The web app (`web/`, built with `pnpm build`) at `/app`. A single-page app: any path that isn't a file gets the
/// shell (`200.html`), except under `/app/_app/`, where a missing file is a real 404. Hashed assets are cached for a
/// year; everything else is revalidated, so a deploy shows up on the next load. Without `dir`, `/app` is a 404.
pub fn web_app(dir: &std::path::Path) -> Router {
    use axum::http::HeaderValue;
    use tower_http::services::{ServeDir, ServeFile};
    use tower_http::set_header::SetResponseHeaderLayer;

    if !dir.join("200.html").is_file() {
        tracing::warn!("no web app at {} (200.html missing): /app is not served", dir.display());
        return Router::new();
    }
    let cache =
        |v: &'static str| SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, HeaderValue::from_static(v));
    let immutable = Router::new()
        .nest_service("/app/_app/immutable", ServeDir::new(dir.join("_app/immutable")))
        .layer(cache("public, max-age=31536000, immutable"));
    let rest = Router::new()
        .nest_service("/app/_app", ServeDir::new(dir.join("_app")))
        .nest_service(
            "/app",
            ServeDir::new(dir).fallback(ServeFile::new(dir.join("200.html"))),
        )
        .layer(cache("no-cache"));
    // The app runs with the session cookie, so no other page may frame it (clickjacking). shortcut: only
    // frame-ancestors, not a full CSP, which would need a hash of the shell's inline theme script.
    let set = |k, v| SetResponseHeaderLayer::overriding(k, HeaderValue::from_static(v));
    immutable
        .merge(rest)
        .layer(set(header::CONTENT_SECURITY_POLICY, "frame-ancestors 'none'"))
        .layer(set(header::X_FRAME_OPTIONS, "DENY"))
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
    async fn web_app_serves_the_shell_and_404s_missing_assets() {
        let dir = std::env::temp_dir().join(format!("health-web-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("_app/immutable")).unwrap();
        std::fs::write(dir.join("200.html"), "shell").unwrap();
        std::fs::write(dir.join("_app/immutable/a1b2.js"), "js").unwrap();
        std::fs::write(dir.join("robots.txt"), "robots").unwrap();
        let get = |path: &'static str| {
            let app = web_app(&dir);
            async move {
                let res = app
                    .oneshot(Request::get(path).body(Body::empty()).unwrap())
                    .await
                    .unwrap();
                let cache = res
                    .headers()
                    .get(header::CACHE_CONTROL)
                    .map(|v| v.to_str().unwrap().to_owned());
                let status = res.status();
                let body = axum::body::to_bytes(res.into_body(), 1 << 16).await.unwrap();
                (status, String::from_utf8(body.to_vec()).unwrap(), cache)
            }
        };
        let no_cache = Some("no-cache".to_owned());
        for path in ["/app", "/app/", "/app/workouts/1"] {
            assert_eq!(
                get(path).await,
                (StatusCode::OK, "shell".into(), no_cache.clone()),
                "{path}"
            );
        }
        // Neither the shell nor a file may be framed by another page.
        for path in ["/app/workouts/1", "/app/robots.txt", "/app/_app/immutable/a1b2.js"] {
            let res = web_app(&dir)
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                res.headers()[header::CONTENT_SECURITY_POLICY],
                "frame-ancestors 'none'",
                "{path}"
            );
            assert_eq!(res.headers()[header::X_FRAME_OPTIONS], "DENY", "{path}");
        }
        assert_eq!(get("/app/robots.txt").await.1, "robots");
        let (status, body, cache) = get("/app/_app/immutable/a1b2.js").await;
        assert_eq!((status, body.as_str()), (StatusCode::OK, "js"));
        assert!(cache.unwrap().contains("immutable"));
        assert_eq!(get("/app/_app/immutable/gone.js").await.0, StatusCode::NOT_FOUND);
        assert_eq!(get("/app/_app/version.json").await.0, StatusCode::NOT_FOUND);
        assert_eq!(get("/other").await.0, StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(&dir).unwrap();
        // No build: nothing is served.
        let res = web_app(std::path::Path::new("/nonexistent"))
            .oneshot(Request::get("/app").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn web_app_requests_are_logged() {
        #[derive(Clone, Default)]
        struct Log(Arc<std::sync::Mutex<Vec<u8>>>);
        impl std::io::Write for Log {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().write(buf)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let log = Log::default();
        let writer = log.clone();
        // Global: with a thread-local set_default the log stayed empty while other tests ran in parallel.
        tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_writer(move || writer.clone())
                .with_ansi(false)
                .finish(),
        )
        .unwrap();
        let dir = std::env::temp_dir().join(format!("health-web-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("200.html"), "shell").unwrap();
        let pool = || sqlx::PgPool::connect_lazy("postgres://localhost:1/none").unwrap();
        let db = db::Db {
            app: pool(),
            reader: pool(),
            gym: Default::default(),
            share: Default::default(),
        };
        let res = app(
            db,
            "health.example.com".into(),
            verifier(MCP),
            verifier(API),
            web_app(&dir),
        )
        .oneshot(Request::get("/app/workouts").body(Body::empty()).unwrap())
        .await
        .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let log = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
        assert!(log.contains(r#"path="/app/workouts""#), "{log}");
    }

    #[tokio::test]
    async fn session_routes_win_over_the_web_app_shell() {
        let dir = std::env::temp_dir().join(format!("health-web-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("200.html"), "shell").unwrap();
        let pool = || sqlx::PgPool::connect_lazy("postgres://localhost/none").unwrap();
        let db = db::Db {
            app: pool(),
            reader: pool(),
            gym: Default::default(),
            share: Default::default(),
        };
        let app = app(
            db,
            "health.example.com".into(),
            verifier(MCP),
            verifier(API),
            web_app(&dir),
        );
        // No web login configured: /app/login answers itself (503), not with the shell.
        let res = app
            .clone()
            .oneshot(Request::get("/app/login").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
        let res = app
            .oneshot(Request::get("/app/settings").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = axum::body::to_bytes(res.into_body(), 1 << 16).await.unwrap();
        assert_eq!(&body[..], b"shell");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn readyz_reports_an_unreachable_database() {
        // Port 1: nothing listens, so connecting fails at once.
        let pool = || sqlx::PgPool::connect_lazy("postgres://localhost:1/none").unwrap();
        let db = db::Db {
            app: pool(),
            reader: pool(),
            gym: Default::default(),
            share: Default::default(),
        };
        let app = app(
            db,
            "health.example.com".into(),
            verifier(MCP),
            verifier(API),
            Router::new(),
        );
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
            share: Default::default(),
        };
        let app = app(
            db,
            "health.example.com".into(),
            verifier(MCP),
            verifier(API),
            Router::new(),
        );
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
            share: Default::default(),
        };
        let app = app(
            db,
            "health.example.com".into(),
            verifier(MCP),
            verifier(API),
            Router::new(),
        );
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
