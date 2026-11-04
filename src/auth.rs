//! OAuth 2.1 resource server: validates RFC 9068 JWT access tokens against the issuer's JWKS.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::RwLock;

/// The authenticated caller: the token's `sub`.
#[derive(Clone, Debug)]
pub struct User(pub String);

pub struct Verifier {
    issuer: String,
    /// Our canonical MCP URL; also the required `aud`.
    resource: String,
    jwks_uri: String,
    http: reqwest::Client,
    jwks: RwLock<(JwkSet, Instant)>,
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
}

const JWKS_MIN_REFRESH: Duration = Duration::from_secs(60);

impl Verifier {
    pub async fn discover(issuer: &str, resource: &str) -> anyhow::Result<Self> {
        let http = reqwest::Client::new();
        #[derive(Deserialize)]
        struct Discovery {
            jwks_uri: String,
        }
        let url = format!("{}/.well-known/openid-configuration", issuer.trim_end_matches('/'));
        let d: Discovery = http.get(url).send().await?.error_for_status()?.json().await?;
        let jwks: JwkSet = http.get(&d.jwks_uri).send().await?.error_for_status()?.json().await?;
        Ok(Self::new(issuer, resource, d.jwks_uri, jwks, http))
    }

    pub fn new(issuer: &str, resource: &str, jwks_uri: String, jwks: JwkSet, http: reqwest::Client) -> Self {
        Self {
            issuer: issuer.to_owned(),
            resource: resource.to_owned(),
            jwks_uri,
            http,
            jwks: RwLock::new((jwks, Instant::now())),
        }
    }

    /// Returns the `sub` of a valid access token.
    pub async fn verify(&self, token: &str) -> Result<String, String> {
        let header = decode_header(token).map_err(|e| e.to_string())?;
        // RFC 9068: rejects ID tokens and other JWTs signed by the same issuer.
        if !matches!(header.typ.as_deref(), Some("at+jwt" | "application/at+jwt")) {
            return Err("not an RFC 9068 access token (typ)".into());
        }
        let kid = header.kid.ok_or("token has no kid")?;
        let key = match self.key(&kid).await {
            Some(k) => k,
            None => {
                self.refresh().await;
                self.key(&kid).await.ok_or("unknown kid")?
            }
        };
        // jsonwebtoken rejects an alg whose family doesn't match the key, so a forged alg can't downgrade.
        let mut v = Validation::new(header.alg);
        v.set_issuer(&[&self.issuer]);
        v.set_audience(&[&self.resource]);
        v.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        v.validate_nbf = true;
        decode::<Claims>(token, &key, &v)
            .map(|d| d.claims.sub)
            .map_err(|e| e.to_string())
    }

    async fn key(&self, kid: &str) -> Option<DecodingKey> {
        let jwks = self.jwks.read().await;
        jwks.0.find(kid).and_then(|j| DecodingKey::from_jwk(j).ok())
    }

    /// Picks up key rotation; rate-limited so random kids can't hammer the IdP.
    async fn refresh(&self) {
        let mut jwks = self.jwks.write().await;
        if jwks.1.elapsed() < JWKS_MIN_REFRESH {
            return;
        }
        jwks.1 = Instant::now();
        match self.fetch_jwks().await {
            Ok(set) => jwks.0 = set,
            Err(e) => tracing::warn!("JWKS refresh failed: {e}"),
        }
    }

    async fn fetch_jwks(&self) -> reqwest::Result<JwkSet> {
        self.http
            .get(&self.jwks_uri)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
    }

    fn metadata_url(&self) -> String {
        // RFC 9728 §3.1: well-known path inserted between host and the resource's path.
        let url = reqwest::Url::parse(&self.resource).expect("resource is a valid URL");
        format!(
            "{}/.well-known/oauth-protected-resource{}",
            url.origin().ascii_serialization(),
            url.path()
        )
    }
}

/// Axum middleware for `/mcp`: 401 with RFC 9728 discovery, or inserts [`User`].
pub async fn require(State(v): State<Arc<Verifier>>, mut req: Request, next: Next) -> Response {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));
    let error = match token {
        None => None,
        Some(t) => match v.verify(t).await {
            Ok(sub) => {
                req.extensions_mut().insert(User(sub));
                return next.run(req).await;
            }
            Err(e) => {
                tracing::debug!("rejected token: {e}");
                Some(r#", error="invalid_token""#)
            }
        },
    };
    let challenge = format!(
        r#"Bearer resource_metadata="{}"{}"#,
        v.metadata_url(),
        error.unwrap_or("")
    );
    (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, challenge)]).into_response()
}

/// RFC 9728 protected resource metadata.
pub async fn metadata(State(v): State<Arc<Verifier>>) -> impl IntoResponse {
    Json(serde_json::json!({
        "resource": v.resource,
        "authorization_servers": [v.issuer],
        "bearer_methods_supported": ["header"],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde_json::json;

    const SECRET: &str = "test-secret-at-least-32-bytes-long!!";
    const ISS: &str = "https://auth.example.com";
    const AUD: &str = "https://vitals.example.com/mcp";

    fn verifier() -> Verifier {
        let k = jsonwebtoken::jwk::Jwk::from_encoding_key(
            &EncodingKey::from_secret(SECRET.as_bytes()),
            jsonwebtoken::Algorithm::HS256,
        )
        .unwrap();
        let mut k = k;
        k.common.key_id = Some("k1".into());
        Verifier::new(
            ISS,
            AUD,
            "http://127.0.0.1:9/jwks".into(),
            JwkSet { keys: vec![k] },
            reqwest::Client::new(),
        )
    }

    fn token(typ: &str, claims: serde_json::Value) -> String {
        let mut h = Header::new(jsonwebtoken::Algorithm::HS256);
        h.typ = Some(typ.into());
        h.kid = Some("k1".into());
        encode(&h, &claims, &EncodingKey::from_secret(SECRET.as_bytes())).unwrap()
    }

    fn claims(iss: &str, aud: &str, exp_offset: i64) -> serde_json::Value {
        let now = chrono::Utc::now().timestamp();
        json!({"iss": iss, "aud": aud, "sub": "alice", "exp": now + exp_offset, "iat": now})
    }

    #[tokio::test]
    async fn accepts_valid_access_token() {
        assert_eq!(
            verifier().verify(&token("at+jwt", claims(ISS, AUD, 300))).await,
            Ok("alice".into())
        );
    }

    #[tokio::test]
    async fn rejects_bad_tokens() {
        let v = verifier();
        for t in [
            token("JWT", claims(ISS, AUD, 300)),                           // ID token / wrong typ
            token("at+jwt", claims("https://evil.example.com", AUD, 300)), // wrong issuer
            token("at+jwt", claims(ISS, "https://other.example.com", 300)), // wrong audience
            token("at+jwt", claims(ISS, AUD, -300)),                       // expired
        ] {
            assert!(v.verify(&t).await.is_err());
        }
    }

    #[test]
    fn metadata_url_inserts_well_known() {
        assert_eq!(
            verifier().metadata_url(),
            "https://vitals.example.com/.well-known/oauth-protected-resource/mcp"
        );
    }
}
