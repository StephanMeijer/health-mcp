//! Browser sessions for the web app at `/app`. The browser logs in with the IdP (the `health-web` client, authorization
//! code with PKCE, the same flow as the invite page) and gets an HttpOnly session cookie. No tokens are kept: after the
//! login the server trusts its own session. `/api` takes either a bearer token (the phone apps) or the cookie (the web
//! app); with the cookie, requests that change something must come from our own origin and say so in a header.

use std::sync::Arc;

use axum::extract::{Query, Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::auth::{Identity, Verifier, authenticate, bearer, insert_identity, unauthorized};
use crate::db::Db;
use crate::web::{challenge, exchange, internal, page, random, read_cookie, valid_code};

/// `__Host-`: the browser only accepts it Secure, without a Domain, on path `/`.
pub const COOKIE: &str = "__Host-session";
/// State, PKCE verifier and where to go after the login, for the 10 minutes a login may take. `__Host-` so a sibling
/// subdomain can't plant its own login (with its own state and verifier) and swap the victim into its account.
const LOGIN_COOKIE: &str = "__Host-health_login";
const LOGIN_MAX_AGE_S: u32 = 600;
/// A session ends 12 h after its last request, and 7 days after the login at the latest.
const IDLE: &str = "12 hours";
const ABSOLUTE_S: u32 = 7 * 24 * 3600;
/// The web app sends this on every API request; a cross-site form or plain fetch can't set it without CORS.
pub const CSRF_HEADER: &str = "x-requested-with";
pub const CSRF_VALUE: &str = "health-web";

#[derive(Clone)]
pub struct Sessions {
    db: Db,
    /// The `/api` verifier: it checks bearer tokens and the token the login returns.
    verifier: Arc<Verifier>,
    /// `PUBLIC_URL` without the trailing slash: what a same-origin request's `Origin` header says.
    origin: String,
}

impl Sessions {
    pub fn new(db: Db, verifier: Arc<Verifier>) -> Self {
        let origin = db.share.public_url.trim_end_matches('/').to_owned();
        Self { db, verifier, origin }
    }
}

pub fn router<S: Clone + Send + Sync + 'static>(s: Sessions) -> Router<S> {
    Router::new()
        .route("/app/login", get(login))
        .route("/app/callback", get(callback))
        .route("/app/logout", post(logout))
        .route("/app/me", get(me))
        .with_state(s)
}

fn hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// The identity behind a session cookie, if the session is live. Refreshes `last_seen` at most once a minute, so
/// a burst of API calls doesn't write a row each.
async fn lookup(db: &Db, token: &str) -> sqlx::Result<Option<Identity>> {
    if !valid_code(token) {
        return Ok(None);
    }
    let row: Option<(String, Option<String>, bool)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT sub, email, last_seen < now() - interval '1 minute' FROM web.session
         WHERE id_hash = $1 AND expires_at > now() AND last_seen > now() - interval '{IDLE}'"
    )))
    .bind(hash(token))
    .fetch_optional(&db.app)
    .await?;
    let Some((sub, email, stale)) = row else {
        return Ok(None);
    };
    if stale {
        sqlx::query("UPDATE web.session SET last_seen = now() WHERE id_hash = $1")
            .bind(hash(token))
            .execute(&db.app)
            .await?;
    }
    Ok(Some(Identity { sub, email }))
}

/// A request that changes something, sent with the cookie, must come from our origin with [`CSRF_HEADER`].
fn same_origin(s: &Sessions, method: &Method, headers: &HeaderMap) -> bool {
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return true;
    }
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    let marked = headers.get(CSRF_HEADER).and_then(|v| v.to_str().ok());
    origin == Some(s.origin.as_str()) && marked == Some(CSRF_VALUE)
}

fn forbidden() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": "cross-site request refused" })),
    )
        .into_response()
}

/// `/api` authentication: a bearer token as before, or else the session cookie.
pub async fn api_auth(State(s): State<Sessions>, mut req: Request, next: Next) -> Response {
    if let Some(t) = bearer(&req) {
        let t = t.to_owned();
        return authenticate(&s.verifier, &t, req, next).await;
    }
    let Some(token) = read_cookie(req.headers(), COOKIE) else {
        return unauthorized(&s.verifier, false);
    };
    if !same_origin(&s, req.method(), req.headers()) {
        return forbidden();
    }
    match lookup(&s.db, &token).await {
        Ok(Some(id)) => {
            insert_identity(&mut req, id);
            next.run(req).await
        }
        Ok(None) => unauthorized(&s.verifier, false),
        Err(e) => {
            tracing::error!("session lookup: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Only paths of the web app: never another origin (`//evil`), never a backslash browsers read as a slash, and only
/// printable ASCII, which is all a `Location` header can hold.
fn safe_next(next: Option<&str>) -> String {
    match next {
        Some(n)
            if (n == "/app" || n.starts_with("/app/"))
                && !n.contains("//")
                && !n.contains('\\')
                && n.bytes().all(|b| b.is_ascii_graphic()) =>
        {
            n.to_owned()
        }
        _ => "/app/".to_owned(),
    }
}

#[derive(Deserialize)]
struct LoginQuery {
    next: Option<String>,
}

fn redirect_uri(s: &Sessions) -> String {
    format!("{}/app/callback", s.origin)
}

async fn login(State(s): State<Sessions>, Query(q): Query<LoginQuery>) -> Response {
    let Some(login) = s.db.share.login.as_deref() else {
        return page(
            StatusCode::SERVICE_UNAVAILABLE,
            "Web login is not set up",
            "<p>This server has no web login client (<code>OIDC_WEB_CLIENT_ID</code>).</p>",
        );
    };
    let (state, verifier) = (random(), random());
    let next = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(safe_next(q.next.as_deref()));
    let Ok(url) = reqwest::Url::parse_with_params(
        &login.authorization_endpoint,
        [
            ("response_type", "code"),
            ("client_id", &login.client_id),
            ("redirect_uri", &redirect_uri(&s)),
            ("scope", "openid email"),
            ("audience", &login.audience),
            ("state", &state),
            ("code_challenge", &challenge(&verifier)),
            ("code_challenge_method", "S256"),
        ],
    ) else {
        return internal("bad authorization endpoint");
    };
    let cookie = format!(
        "{LOGIN_COOKIE}={state}.{verifier}.{next}; Path=/; Max-Age={LOGIN_MAX_AGE_S}; HttpOnly; Secure; SameSite=Lax"
    );
    ([(header::SET_COOKIE, cookie)], Redirect::to(url.as_str())).into_response()
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// `(state, verifier, next)` from the login cookie.
fn login_cookie(headers: &HeaderMap) -> Option<(String, String, String)> {
    let value = read_cookie(headers, LOGIN_COOKIE)?;
    let mut it = value.split('.');
    let (state, verifier, next) = (it.next()?, it.next()?, it.next()?);
    let next = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(next).ok()?;
    let next = String::from_utf8(next).ok()?;
    (it.next().is_none() && valid_code(state) && valid_code(verifier))
        .then(|| (state.into(), verifier.into(), safe_next(Some(&next))))
}

async fn callback(State(s): State<Sessions>, headers: HeaderMap, Query(q): Query<CallbackQuery>) -> Response {
    let clear = format!("{LOGIN_COOKIE}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax");
    let mut res = finish_login(&s, &headers, q).await;
    res.headers_mut()
        .append(header::SET_COOKIE, clear.parse().expect("ascii"));
    res
}

async fn finish_login(s: &Sessions, headers: &HeaderMap, q: CallbackQuery) -> Response {
    let Some(login) = s.db.share.login.as_deref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let again = "<p><a href=\"/app/login\">Log in again</a>.</p>";
    let Some((state, verifier, next)) = login_cookie(headers) else {
        return page(StatusCode::BAD_REQUEST, "Login expired", again);
    };
    if q.state.as_deref() != Some(state.as_str()) {
        return page(StatusCode::BAD_REQUEST, "Login failed", again);
    }
    if q.error.is_some() {
        return page(StatusCode::BAD_REQUEST, "Login cancelled", again);
    }
    let Some(code) = q.code else {
        return page(StatusCode::BAD_REQUEST, "Login failed", again);
    };
    let id = match exchange(login, &s.verifier, &code, &verifier, &redirect_uri(s)).await {
        Ok(id) => id,
        Err(e) => return internal(format!("token exchange: {e}")),
    };
    let token = random();
    let created = async {
        // Expired sessions go here, on login, rather than in a background job.
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM web.session WHERE expires_at < now() OR last_seen < now() - interval '{IDLE}'"
        )))
        .execute(&s.db.app)
        .await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO web.session (id_hash, sub, email, expires_at)
             VALUES ($1, $2, $3, now() + interval '{ABSOLUTE_S} seconds')"
        )))
        .bind(hash(&token))
        .bind(&id.sub)
        .bind(&id.email)
        .execute(&s.db.app)
        .await
    };
    if let Err(e) = created.await {
        return internal(format!("create session: {e}"));
    }
    let cookie = format!("{COOKIE}={token}; Path=/; Max-Age={ABSOLUTE_S}; HttpOnly; Secure; SameSite=Lax");
    ([(header::SET_COOKIE, cookie)], Redirect::to(&next)).into_response()
}

async fn logout(State(s): State<Sessions>, method: Method, headers: HeaderMap) -> Response {
    if !same_origin(&s, &method, &headers) {
        return forbidden();
    }
    if let Some(token) = read_cookie(&headers, COOKIE)
        && let Err(e) = sqlx::query("DELETE FROM web.session WHERE id_hash = $1")
            .bind(hash(&token))
            .execute(&s.db.app)
            .await
    {
        return internal(format!("logout: {e}"));
    }
    let clear = format!("{COOKIE}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax");
    (StatusCode::NO_CONTENT, [(header::SET_COOKIE, clear)]).into_response()
}

/// Who is logged in, and whose logbooks they can open (the web app's logbook switcher).
async fn me(State(s): State<Sessions>, headers: HeaderMap) -> Response {
    let id = match read_cookie(&headers, COOKIE) {
        None => None,
        Some(token) => match lookup(&s.db, &token).await {
            Ok(id) => id,
            Err(e) => return internal(format!("session lookup: {e}")),
        },
    };
    let Some(id) = id else {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "not logged in" }))).into_response();
    };
    let shared: Result<serde_json::Value, _> = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('owner', owner_email, 'gym', gym, 'vitals', vitals)
                                   ORDER BY owner_email), '[]') FROM share.access WHERE grantee_sub = $1",
    )
    .bind(&id.sub)
    .fetch_one(&s.db.app)
    .await;
    match shared {
        Ok(shared) => Json(json!({ "sub": id.sub, "email": id.email, "shared_with_you": shared })).into_response(),
        Err(e) => internal(format!("me: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_stays_inside_the_app() {
        assert_eq!(safe_next(Some("/app/workouts/3?tab=sets")), "/app/workouts/3?tab=sets");
        assert_eq!(safe_next(Some("/app")), "/app");
        for bad in [
            None,
            Some(""),
            Some("/"),
            Some("//evil.example"),
            Some("/app//evil"),
            Some("/app\\x"),
            Some("https://evil.example/app/"),
            Some("/apple"),
            Some("/app/café"),
            Some("/app/\n"),
            Some("/app/a\tb"),
            Some("/app/a b"),
            Some("/app/\u{7f}"),
        ] {
            assert_eq!(safe_next(bad), "/app/", "{bad:?}");
        }
    }
}
