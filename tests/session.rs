//! The web app's browser login and session cookie, through the real routes with a fake IdP token endpoint.
//! Needs the dev database: `docker compose up -d`.

mod common;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use common::*;
use health_mcp::db::Db;
use serde_json::{Value, json};
use tower::ServiceExt;

const ORIGIN: &str = "https://health.example.com";

async fn send(db: &Db, req: Request<Body>) -> (StatusCode, HeaderMap, Value) {
    let res = app(db).oneshot(req).await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let body = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    (status, headers, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

/// `name=value` of the first Set-Cookie for `name`.
fn set_cookie(h: &HeaderMap, name: &str) -> String {
    h.get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap())
        .find(|c| c.starts_with(&format!("{name}=")))
        .unwrap_or_else(|| panic!("no {name} cookie in {h:?}"))
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

/// The whole first Set-Cookie for `name`, attributes included.
fn cookie_attrs(h: &HeaderMap, name: &str) -> String {
    h.get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .find(|c| c.starts_with(&format!("{name}=")))
        .unwrap_or_else(|| panic!("no {name} cookie in {h:?}"))
}

/// Logs in through /app/login and /app/callback; returns the session cookie and where the callback sent us.
async fn log_in(db: &Db, next: &str) -> (String, String) {
    let (s, h, _) = send(
        db,
        Request::get(format!("/app/login?next={next}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::SEE_OTHER);
    let to = reqwest::Url::parse(h[header::LOCATION].to_str().unwrap()).unwrap();
    let q: std::collections::HashMap<_, _> = to.query_pairs().into_owned().collect();
    assert_eq!(q["redirect_uri"], format!("{ORIGIN}/app/callback"));
    assert_eq!(q["code_challenge_method"], "S256");
    let login = set_cookie(&h, "__Host-health_login");
    let attrs = cookie_attrs(&h, "__Host-health_login");
    for a in ["Path=/;", "HttpOnly", "Secure", "SameSite=Lax"] {
        assert!(attrs.contains(a), "{a} in {attrs}");
    }
    let (s, h, _) = send(
        db,
        Request::get(format!("/app/callback?code=the-code&state={}", q["state"]))
            .header(header::COOKIE, &login)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::SEE_OTHER, "callback");
    // Cleared on the same path it was set on, or the browser keeps it.
    assert!(cookie_attrs(&h, "__Host-health_login").starts_with("__Host-health_login=; Path=/;"));
    let session = set_cookie(&h, "__Host-session");
    let attrs = cookie_attrs(&h, "__Host-session");
    assert!(
        attrs.contains("HttpOnly") && attrs.contains("Secure") && attrs.contains("Path=/;"),
        "{attrs}"
    );
    (session, h[header::LOCATION].to_str().unwrap().to_owned())
}

/// A tool call from the web app: the session cookie plus the same-origin markers.
fn tool_req(cookie: &str, name: &str, origin: Option<&str>, marked: bool) -> Request<Body> {
    let mut r = Request::post(format!("/api/v1/tools/{name}"))
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(o) = origin {
        r = r.header(header::ORIGIN, o);
    }
    if marked {
        r = r.header("x-requested-with", "health-web");
    }
    r.body(Body::from("{}")).unwrap()
}

#[tokio::test]
async fn login_gives_a_cookie_that_works_on_the_api() {
    let who = person("web");
    let db = db(Some(web_login(&who).await)).await;
    let (cookie, landed) = log_in(&db, "/app/workouts/3").await;
    assert_eq!(landed, "/app/workouts/3");

    let (s, h, me) = send(
        &db,
        Request::get("/app/me")
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h[header::CONTENT_SECURITY_POLICY], "frame-ancestors 'none'");
    assert_eq!(h[header::X_FRAME_OPTIONS], "DENY");
    assert_eq!(
        (me["sub"].as_str(), me["email"].as_str()),
        (Some(who.sub.as_str()), who.email.as_deref())
    );
    assert_eq!(me["shared_with_you"], json!([]));

    // The tools run as the session's user, with its verified email (sharing needs it).
    let (s, _, out) = send(&db, tool_req(&cookie, "share_list", Some(ORIGIN), true)).await;
    assert_eq!(s, StatusCode::OK, "{out}");
    assert_eq!(out["your_email"].as_str(), who.email.as_deref());
    let (s, _, _) = send(&db, tool_req(&cookie, "gym_list", Some(ORIGIN), true)).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn cookie_requests_must_be_same_origin() {
    let who = person("web");
    let db = db(Some(web_login(&who).await)).await;
    let (cookie, _) = log_in(&db, "/app/").await;
    for (origin, marked) in [
        (None, true),
        (Some("https://evil.example"), true),
        (Some(ORIGIN), false),
    ] {
        let (s, _, _) = send(&db, tool_req(&cookie, "gym_list", origin, marked)).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{origin:?} {marked}");
    }
    // No cookie and no token: the usual 401 with discovery.
    let r = Request::post("/api/v1/tools/gym_list").body(Body::from("{}")).unwrap();
    let (s, h, _) = send(&db, r).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(h.contains_key(header::WWW_AUTHENTICATE));
    // A made-up cookie: 401.
    let (s, _, _) = send(
        &db,
        tool_req(
            &format!("__Host-session={}", "a".repeat(64)),
            "gym_list",
            Some(ORIGIN),
            true,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sessions_expire_and_log_out() {
    let who = person("web");
    let db = db(Some(web_login(&who).await)).await;
    let ok = |cookie: String| {
        let db = db.clone();
        async move { send(&db, tool_req(&cookie, "gym_list", Some(ORIGIN), true)).await.0 }
    };

    // Idle for longer than 12 hours.
    let (idle, _) = log_in(&db, "/app/").await;
    sqlx::query("UPDATE web.session SET last_seen = now() - interval '13 hours' WHERE sub = $1")
        .bind(&who.sub)
        .execute(&db.app)
        .await
        .unwrap();
    assert_eq!(ok(idle).await, StatusCode::UNAUTHORIZED);

    // Past the absolute end.
    let (old, _) = log_in(&db, "/app/").await;
    sqlx::query("UPDATE web.session SET expires_at = now() - interval '1 second' WHERE sub = $1")
        .bind(&who.sub)
        .execute(&db.app)
        .await
        .unwrap();
    assert_eq!(ok(old).await, StatusCode::UNAUTHORIZED);

    // Logout needs the same-origin markers too, then ends the session.
    let (cookie, _) = log_in(&db, "/app/").await;
    assert_eq!(ok(cookie.clone()).await, StatusCode::OK);
    let logout = |marked: bool| {
        let mut r = Request::post("/app/logout")
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, ORIGIN);
        if marked {
            r = r.header("x-requested-with", "health-web");
        }
        r.body(Body::empty()).unwrap()
    };
    assert_eq!(send(&db, logout(false)).await.0, StatusCode::FORBIDDEN);
    let (s, h, _) = send(&db, logout(true)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(set_cookie(&h, "__Host-session").ends_with('='), "cookie cleared");
    assert_eq!(ok(cookie).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_only_returns_into_the_app() {
    let who = person("web");
    let db = db(Some(web_login(&who).await)).await;
    for next in ["//evil.example", "https%3A%2F%2Fevil.example%2Fapp%2F", "/apple"] {
        assert_eq!(log_in(&db, next).await.1, "/app/", "{next}");
    }
    // A callback whose state isn't ours, or without the login cookie, logs nobody in.
    let (s, h, _) = send(&db, Request::get("/app/login").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::SEE_OTHER);
    let login = set_cookie(&h, "__Host-health_login");
    let state = cookie_attrs(&h, "__Host-health_login")
        .split(['=', '.'])
        .nth(1)
        .unwrap()
        .to_owned();
    // The same value under a name without the prefix (what a sibling subdomain could set) is not the login cookie.
    let tossed = Request::get(format!("/app/callback?code=the-code&state={state}"))
        .header(header::COOKIE, login.replacen("__Host-", "", 1))
        .body(Body::empty())
        .unwrap();
    let (s, h, _) = send(&db, tossed).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(
        !h.get_all(header::SET_COOKIE)
            .iter()
            .any(|c| c.to_str().unwrap().starts_with("__Host-session="))
    );
    let wrong = Request::get(format!("/app/callback?code=the-code&state={}", "f".repeat(64)))
        .header(header::COOKIE, &login)
        .body(Body::empty())
        .unwrap();
    let (s, h, _) = send(&db, wrong).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(
        !h.get_all(header::SET_COOKIE)
            .iter()
            .any(|c| c.to_str().unwrap().starts_with("__Host-session="))
    );
    let no_cookie = Request::get("/app/callback?code=the-code&state=x")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&db, no_cookie).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn cookie_sessions_keep_read_only_shares_read_only() {
    let g = person("grantee");
    let db = db(Some(web_login(&g).await)).await;
    let o = owner_with_data(&db).await;
    let (id, _) = invite(&db, &o, &g, "ro", "none").await;
    ok(&db, &g, "share_respond", json!({"invite_id": id, "accept": true})).await;
    let (cookie, _) = log_in(&db, "/app/").await;
    let call = |name: &str, args: Value| {
        Request::post(format!("/api/v1/tools/{name}"))
            .header(header::COOKIE, &cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, ORIGIN)
            .header("x-requested-with", "health-web")
            .body(Body::from(args.to_string()))
            .unwrap()
    };
    let (s, _, out) = send(&db, call("gym_list", json!({"owner": o.email()}))).await;
    assert!(s == StatusCode::OK && out.to_string().contains("Home"), "{out}");
    let (s, _, out) = send(&db, call("gym_create", json!({"owner": o.email(), "name": "Mine"}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{out}");
    assert!(out["error"].as_str().unwrap().contains("read-only"), "{out}");
}
