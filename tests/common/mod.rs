//! Helpers for the end-to-end tests: the real routes with signed test tokens. Needs the dev database.
#![allow(dead_code, reason = "each test binary uses a different part")]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, header};
use health_mcp::auth::Verifier;
use health_mcp::db::{Db, ShareConfig};
use health_mcp::web::WebLogin;
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

pub const SECRET: &str = "test-secret-at-least-32-bytes-long!!";
pub const ISS: &str = "https://auth.example.com";
pub const PUBLIC: &str = "https://health.example.com/";
pub const MCP: &str = "https://health.example.com/mcp";
pub const API: &str = "https://health.example.com/api";

pub fn url(var: &str, default: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| default.into())
}

pub fn verifier(aud: &str) -> Arc<Verifier> {
    let mut k = Jwk::from_encoding_key(
        &EncodingKey::from_secret(SECRET.as_bytes()),
        jsonwebtoken::Algorithm::HS256,
    )
    .unwrap();
    k.common.key_id = Some("k1".into());
    Arc::new(Verifier::new(
        ISS,
        aud,
        "http://127.0.0.1:9/jwks".into(),
        JwkSet { keys: vec![k] },
        reqwest::Client::new(),
    ))
}

/// An access token for `who`, with `email` verified unless it is `None`.
pub fn token(aud: &str, who: &Person) -> String {
    let mut h = Header::new(jsonwebtoken::Algorithm::HS256);
    h.typ = Some("at+jwt".into());
    h.kid = Some("k1".into());
    let now = chrono::Utc::now().timestamp();
    let mut c = json!({"iss": ISS, "aud": aud, "sub": who.sub, "exp": now + 300});
    if let Some(e) = &who.email {
        c["email"] = json!(e);
        c["email_verified"] = json!(true);
    }
    encode(&h, &c, &EncodingKey::from_secret(SECRET.as_bytes())).unwrap()
}

#[derive(Clone)]
pub struct Person {
    pub sub: String,
    pub email: Option<String>,
}

pub fn person(name: &str) -> Person {
    let id = uuid::Uuid::new_v4();
    Person {
        sub: format!("{name}-{id}"),
        email: Some(format!("{name}-{id}@example.com")),
    }
}

impl Person {
    pub fn email(&self) -> &str {
        self.email.as_deref().unwrap()
    }
}

pub async fn db(login: Option<WebLogin>) -> Db {
    let owner = PgPool::connect(&url(
        "TEST_OWNER_URL",
        "postgres://postgres:postgres@localhost:55432/health",
    ))
    .await
    .unwrap();
    sqlx::migrate!().run(&owner).await.unwrap();
    Db {
        app: PgPool::connect(&url("TEST_APP_URL", "postgres://vitals_app:app@localhost:55432/health"))
            .await
            .unwrap(),
        reader: health_mcp::db::reader_pool_options()
            .connect(&url(
                "TEST_READER_URL",
                "postgres://vitals_reader:reader@localhost:55432/health",
            ))
            .await
            .unwrap(),
        gym: Default::default(),
        share: ShareConfig {
            public_url: PUBLIC.into(),
            mailer: None,
            login: login.map(Arc::new),
        },
    }
}

pub fn app(db: &Db) -> axum::Router {
    health_mcp::app(db.clone(), "health.example.com".into(), verifier(MCP), verifier(API))
}

/// Calls `/api/v1/tools/{name}` as `who`; returns status and JSON body.
pub async fn tool(db: &Db, who: &Person, name: &str, args: Value) -> (u16, Value) {
    let req = Request::post(format!("/api/v1/tools/{name}"))
        .header(header::AUTHORIZATION, format!("Bearer {}", token(API, who)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(args.to_string()))
        .unwrap();
    let res = app(db).oneshot(req).await.unwrap();
    let status = res.status().as_u16();
    let body = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

pub async fn ok(db: &Db, who: &Person, name: &str, args: Value) -> Value {
    let (s, out) = tool(db, who, name, args).await;
    assert_eq!(s, 200, "{name}: {out}");
    out
}

pub async fn err(db: &Db, who: &Person, name: &str, args: Value) -> String {
    let (s, out) = tool(db, who, name, args).await;
    assert_eq!(s, 400, "{name} should fail: {out}");
    out["error"].as_str().unwrap().to_owned()
}

/// Invites `to` and returns the invite id and code (from the link: no mailer in tests).
pub async fn invite(db: &Db, from: &Person, to: &Person, gym: &str, vitals: &str) -> (i64, String) {
    let out = ok(
        db,
        from,
        "share_invite",
        json!({"email": to.email().to_uppercase(), "gym": gym, "vitals": vitals}),
    )
    .await;
    assert_eq!(out["mailed"], false);
    let code = out["link"].as_str().unwrap().rsplit('/').next().unwrap().to_owned();
    let list = ok(db, to, "share_list", json!({})).await;
    let id = list["invites_received"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["from"] == from.email())
        .expect("invite listed for the invitee")["invite_id"]
        .as_i64()
        .unwrap();
    (id, code)
}

/// Owner `o` with one gym, exercise, workout and set, plus a heart rate reading.
pub async fn owner_with_data(db: &Db) -> Person {
    let o = person("owner");
    ok(db, &o, "gym_create", json!({"name": "Home"})).await;
    ok(
        db,
        &o,
        "exercise_create",
        json!({"name": "Squat", "muscle_groups": ["quads"]}),
    )
    .await;
    let w = ok(
        db,
        &o,
        "workout_add_past",
        json!({"gym": "Home", "started_at": "2026-10-01T18:00:00Z", "finished_at": "2026-10-01T19:00:00Z"}),
    )
    .await;
    ok(
        db,
        &o,
        "set_add",
        json!({"workout_id": w["workout_id"], "exercise": "Squat", "sets": [{"reps": 5, "weight_kg": 100, "performed_at": "2026-10-01T18:10:00Z"}]}),
    )
    .await;
    ok(
        db,
        &o,
        "reading_add",
        json!({"kind": "heart_rate", "readings": [{"time": "2026-10-01T07:00:00Z", "bpm": 55}]}),
    )
    .await;
    o
}
