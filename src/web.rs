//! The invite pages. `/invite/{code}` shows who shares what, with Accept and Decline. Accept logs in with the IdP
//! (authorization code with PKCE, a confidential client of its own) and comes back to `/invite/callback`, which
//! verifies the access token like `/api` does and accepts as that user. No session: the login state lives in one
//! short-lived cookie, and the page is done once the invite is answered.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::auth::{Identity, Verifier};
use crate::db::Db;
use crate::share::{self, Invite};
use crate::tools::ToolError;

/// The IdP client the invite pages log in with (`OIDC_WEB_CLIENT_ID` / `OIDC_WEB_CLIENT_SECRET`).
pub struct WebLogin {
    pub client_id: String,
    pub client_secret: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    /// `{PUBLIC_URL}invite/callback`.
    pub redirect_uri: String,
    /// The `/api` resource: the token must pass the same checks as an app token.
    pub audience: String,
    pub http: reqwest::Client,
}

#[derive(Clone)]
struct Web {
    db: Db,
    verifier: Arc<Verifier>,
}

impl Web {
    fn login(&self) -> Option<&WebLogin> {
        self.db.share.login.as_deref()
    }
}

/// `__Host-`: Secure, `Path=/`, no `Domain`, so a sibling subdomain can't plant it.
const COOKIE: &str = "__Host-health_invite";
/// Long enough to log in (including a second factor), short enough not to linger.
const COOKIE_MAX_AGE_S: u32 = 600;

/// `verifier` checks the access token the login returns: pass the `/api` one. Without `db.share.login`, the page
/// says to accept in the app or Claude.
pub fn router<S: Clone + Send + Sync + 'static>(db: Db, verifier: Arc<Verifier>) -> Router<S> {
    Router::new()
        .route("/invite/callback", get(callback))
        .route("/invite/{code}", get(show))
        .route("/invite/{code}/accept", post(accept))
        .route("/invite/{code}/decline", post(decline))
        .with_state(Web { db, verifier })
}

pub(crate) fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// `body` is trusted HTML; escape anything from users with [`esc`].
pub(crate) fn page(status: StatusCode, title: &str, body: &str) -> Response {
    let html = format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1"><title>{t}</title>
<style>
:root {{ color-scheme: light dark; --fg: #1d1d1f; --bg: #fafafa; --card: #fff; --muted: #6e6e73; --accent: #0a7d5a; }}
@media (prefers-color-scheme: dark) {{ :root {{ --fg: #f2f2f2; --bg: #151517; --card: #1f1f22; --muted: #a1a1a6; --accent: #3ccf9b; }} }}
body {{ margin: 0; padding: 16px; font: 16px/1.5 system-ui, sans-serif; color: var(--fg); background: var(--bg); }}
main {{ max-width: 32rem; margin: 10vh auto; padding: 24px; background: var(--card); border-radius: 12px; }}
h1 {{ font-size: 1.3rem; margin-top: 0; }} .muted {{ color: var(--muted); font-size: .9rem; }}
dl {{ display: grid; grid-template-columns: auto 1fr; gap: 4px 16px; }} dt {{ color: var(--muted); }} dd {{ margin: 0; }}
form {{ display: inline; }} button {{ font: inherit; padding: 8px 18px; border-radius: 8px; border: 1px solid var(--muted);
  background: none; color: var(--fg); cursor: pointer; margin-right: 8px; }}
button.primary {{ background: var(--accent); border-color: var(--accent); color: #fff; }}
</style></head><body><main><h1>{t}</h1>{body}</main></body></html>"#,
        t = esc(title)
    );
    (status, Html(html)).into_response()
}

fn levels(inv: &Invite) -> String {
    format!(
        "<dl><dt>Gym logbook</dt><dd>{}</dd><dt>Vitals</dt><dd>{}</dd></dl>",
        inv.gym.describe(),
        inv.vitals.describe()
    )
}

fn gone() -> Response {
    page(
        StatusCode::NOT_FOUND,
        "Invite not found",
        "<p>This invite was already answered, withdrawn, or has expired. Ask for a new one if you still need it.</p>",
    )
}

pub(crate) fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("invite page: {e}");
    page(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Something went wrong",
        "<p>Please try again later.</p>",
    )
}

/// Codes are 64 hex characters; anything else can't match, so don't touch the database for it.
pub(crate) fn valid_code(code: &str) -> bool {
    code.len() == 64 && code.bytes().all(|b| b.is_ascii_hexdigit())
}

async fn show(State(w): State<Web>, Path(code): Path<String>) -> Response {
    if !valid_code(&code) {
        return gone();
    }
    let inv = match share::invite_by_code(&w.db, &code).await {
        Ok(Some(i)) => i,
        Ok(None) => return gone(),
        Err(e) => return internal(e),
    };
    let accept = match w.login() {
        Some(_) => format!(
            r#"<form method="post" action="/invite/{code}/accept"><button class="primary">Accept</button></form>"#
        ),
        None => String::new(),
    };
    let how = match w.login() {
        Some(_) => format!("Accepting asks you to log in with <b>{}</b>.", esc(&inv.email)),
        None => format!(
            "To accept, log in to the health app or Claude with <b>{}</b> and accept the invite there.",
            esc(&inv.email)
        ),
    };
    page(
        StatusCode::OK,
        "Health data shared with you",
        &format!(
            "<p><b>{owner}</b> wants to share their health data with you.</p>{levels}<p>{how}</p>\
             <p>{accept}<form method=\"post\" action=\"/invite/{code}/decline\"><button>Decline</button></form></p>\
             <p class=\"muted\">The invite expires on {exp}.</p>",
            owner = esc(&inv.owner_email),
            levels = levels(&inv),
            exp = inv.expires_at.format("%Y-%m-%d %H:%M UTC"),
        ),
    )
}

/// The code is the secret, as in the mail: declining needs no login.
async fn decline(State(w): State<Web>, Path(code): Path<String>) -> Response {
    if !valid_code(&code) {
        return gone();
    }
    match share::decline_code(&w.db, &code).await {
        Ok(true) => page(
            StatusCode::OK,
            "Invite declined",
            "<p>Nothing was shared. You can close this page.</p>",
        ),
        Ok(false) => gone(),
        Err(e) => internal(e),
    }
}

/// 64 hex characters from the OS CSPRNG, for `state` and the PKCE verifier.
pub(crate) fn random() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// RFC 7636 S256.
pub(crate) fn challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

async fn accept(State(w): State<Web>, Path(code): Path<String>) -> Response {
    let Some(login) = w.login() else {
        return page(
            StatusCode::NOT_FOUND,
            "Accept in the app",
            "<p>Accept the invite in the health app or in Claude.</p>",
        );
    };
    if !valid_code(&code) {
        return gone();
    }
    let (state, verifier) = (random(), random());
    let Ok(url) = reqwest::Url::parse_with_params(
        &login.authorization_endpoint,
        [
            ("response_type", "code"),
            ("client_id", &login.client_id),
            ("redirect_uri", &login.redirect_uri),
            ("scope", "openid email"),
            ("audience", &login.audience),
            ("state", &state),
            ("code_challenge", &challenge(&verifier)),
            ("code_challenge_method", "S256"),
        ],
    ) else {
        return internal("bad authorization endpoint");
    };
    // Lax: sent on the IdP's top-level redirect back to the callback, not on cross-site subrequests.
    let cookie = format!(
        "{COOKIE}={state}.{verifier}.{code}; Path=/; Max-Age={COOKIE_MAX_AGE_S}; HttpOnly; Secure; SameSite=Lax"
    );
    ([(header::SET_COOKIE, cookie)], Redirect::to(url.as_str())).into_response()
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// The value of cookie `name` in `headers`.
pub(crate) fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|h| h.split(';'))
        .find_map(|c| c.trim().strip_prefix(&prefix).map(str::to_owned))
}

/// `(state, verifier, invite code)` from the login cookie.
fn login_cookie(headers: &HeaderMap) -> Option<(String, String, String)> {
    let value = read_cookie(headers, COOKIE)?;
    let mut it = value.split('.');
    let (s, v, c) = (it.next()?, it.next()?, it.next()?);
    (it.next().is_none() && valid_code(s) && valid_code(v) && valid_code(c)).then(|| (s.into(), v.into(), c.into()))
}

async fn callback(State(w): State<Web>, headers: HeaderMap, Query(q): Query<CallbackQuery>) -> Response {
    let clear = format!("{COOKIE}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax");
    let res = finish_login(&w, &headers, q).await;
    ([(header::SET_COOKIE, clear)], res).into_response()
}

async fn finish_login(w: &Web, headers: &HeaderMap, q: CallbackQuery) -> Response {
    let Some(login) = w.login() else {
        return gone();
    };
    let Some((state, verifier, invite)) = login_cookie(headers) else {
        return page(
            StatusCode::BAD_REQUEST,
            "Login expired",
            "<p>The login took too long or came from another browser. Open the invite link again.</p>",
        );
    };
    if q.state.as_deref() != Some(state.as_str()) {
        return page(
            StatusCode::BAD_REQUEST,
            "Login failed",
            "<p>Open the invite link again.</p>",
        );
    }
    if let Some(e) = q.error {
        return page(
            StatusCode::BAD_REQUEST,
            "Login cancelled",
            &format!(
                "<p>The login did not complete ({}). Open the invite link to try again.</p>",
                esc(&e)
            ),
        );
    }
    let Some(code) = q.code else {
        return page(
            StatusCode::BAD_REQUEST,
            "Login failed",
            "<p>Open the invite link again.</p>",
        );
    };
    let identity = match exchange(login, &w.verifier, &code, &verifier, &login.redirect_uri).await {
        Ok(i) => i,
        Err(e) => return internal(format!("token exchange: {e}")),
    };
    let Some(email) = identity.email else {
        return page(
            StatusCode::FORBIDDEN,
            "No verified email",
            "<p>Your account has no verified email address, so the invite can't be matched to you.</p>",
        );
    };
    let inv = match share::invite_by_code(&w.db, &invite).await {
        Ok(Some(i)) => i,
        Ok(None) => return gone(),
        Err(e) => return internal(e),
    };
    match share::respond(&w.db, inv.id, &identity.sub, &email, true).await {
        Ok(inv) => page(
            StatusCode::OK,
            "Invite accepted",
            &format!(
                "<p>You can now see the health data of <b>{}</b>:</p>{}\
                 <p>In Claude or the health app, ask about their data by name or email.</p>",
                esc(&inv.owner_email),
                levels(&inv)
            ),
        ),
        Err(ToolError::User(m)) => page(StatusCode::FORBIDDEN, "Can't accept", &format!("<p>{}</p>", esc(&m))),
        Err(e) => internal(e),
    }
}

/// Trades the authorization code for an access token and checks it like an app token.
pub(crate) async fn exchange(
    login: &WebLogin,
    verifier: &Verifier,
    code: &str,
    pkce: &str,
    redirect_uri: &str,
) -> anyhow::Result<Identity> {
    #[derive(Deserialize)]
    struct Token {
        access_token: String,
    }
    let t: Token = login
        .http
        .post(&login.token_endpoint)
        .basic_auth(&login.client_id, Some(&login.client_secret))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", pkce),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    verifier.identify(&t.access_token).await.map_err(anyhow::Error::msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_matches_rfc_7636_appendix_b() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn escapes_html() {
        assert_eq!(esc(r#"<a href="x">&'"#), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
    }

    #[test]
    fn reads_the_login_cookie() {
        let (s, v, c) = (random(), random(), random());
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            format!("other=1; {COOKIE}={s}.{v}.{c}").parse().unwrap(),
        );
        assert_eq!(login_cookie(&h), Some((s.clone(), v.clone(), c.clone())));
        h.insert(header::COOKIE, format!("{COOKIE}={s}.{v}").parse().unwrap());
        assert_eq!(login_cookie(&h), None);
        h.insert(header::COOKIE, format!("{COOKIE}={s}.{v}.<script>").parse().unwrap());
        assert_eq!(login_cookie(&h), None);
        h.insert(header::COOKIE, format!("health_invite={s}.{v}.{c}").parse().unwrap());
        assert_eq!(login_cookie(&h), None, "only the __Host- name counts");
    }
}
