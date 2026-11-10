//! Sharing: an owner gives another user read (`ro`) or read-write (`rw`) access to their gym and/or vitals data.
//!
//! Invites go by email and are bound to the grantee's verified email. A grantee then passes `owner` to a data tool:
//! [`act_as`] checks the share and swaps the owner in as the tool's user, so the tools themselves stay unaware of it.

use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;

use axum::http::request::Parts;
use chrono::{DateTime, Utc};
use rmcp::handler::server::{tool::Extension, wrapper::Parameters};
use rmcp::{schemars, tool, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::{User, VerifiedEmail};
use crate::db::Db;
use crate::tools::{Health, ToolError, ToolResult, db_err, user};

/// How much of one domain an owner shares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    #[default]
    None,
    /// Read only.
    Ro,
    /// Read and write.
    Rw,
}

impl Access {
    pub fn as_str(self) -> &'static str {
        match self {
            Access::None => "none",
            Access::Ro => "ro",
            Access::Rw => "rw",
        }
    }

    /// The database CHECK allows only these three.
    fn parse(s: &str) -> Self {
        match s {
            "ro" => Access::Ro,
            "rw" => Access::Rw,
            _ => Access::None,
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Access::None => "not shared",
            Access::Ro => "read-only",
            Access::Rw => "read and write",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Domain {
    Gym,
    Vitals,
}

impl fmt::Display for Domain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Domain::Gym => "gym",
            Domain::Vitals => "vitals",
        })
    }
}

/// The domains sql_query may read, signed into the reader transaction (migration 0011).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Domains {
    pub gym: bool,
    pub vitals: bool,
}

impl Domains {
    pub const ALL: Domains = Domains {
        gym: true,
        vitals: true,
    };
}

impl fmt::Display for Domains {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = [(self.gym, "gym"), (self.vitals, "vitals")]
            .into_iter()
            .filter_map(|(on, n)| on.then_some(n))
            .collect();
        f.write_str(&names.join(","))
    }
}

/// What a tool needs from a share to run on someone else's data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Scope {
    /// Only on your own account: sharing itself, schema_describe.
    Own,
    /// A typed tool of one domain; `true` if it writes.
    Data(Domain, bool),
    /// sql_query: whatever domains are shared, read-only.
    Sql,
}

/// Every tool's scope: the domain from the router that holds it, write from its `read_only_hint`.
static SCOPES: LazyLock<HashMap<String, Scope>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    for (router, domain) in [
        (Health::tool_router(), Domain::Vitals),
        (Health::gym_router(), Domain::Gym),
    ] {
        for t in router.list_all() {
            let read_only = t.annotations.as_ref().and_then(|a| a.read_only_hint) == Some(true);
            m.insert(t.name.to_string(), Scope::Data(domain, !read_only));
        }
    }
    for t in Health::share_router().list_all() {
        m.insert(t.name.to_string(), Scope::Own);
    }
    m.insert("schema_describe".into(), Scope::Own);
    m.insert("sql_query".into(), Scope::Sql);
    m
});

/// `None` for an unknown tool.
pub(crate) fn scope(tool: &str) -> Option<Scope> {
    SCOPES.get(tool).copied()
}

pub(crate) const OWNER_DOC: &str = "Email of someone who shared their data with you (see share_list): run this on their data instead of your own. Omit for your own.";

/// The tool call runs as `user`, seeing only `domains` in sql_query.
pub(crate) struct Acting {
    pub user: User,
    pub domains: Domains,
}

/// Takes `owner` out of `args` and checks the caller may run `tool` on that owner's data.
/// `Ok(None)`: no `owner` (or the caller's own), or an unknown tool, which the router then reports.
pub(crate) async fn act_as(
    db: &Db,
    caller: &User,
    email: Option<&VerifiedEmail>,
    tool: &str,
    args: &mut Map<String, Value>,
) -> Result<Option<Acting>, ToolError> {
    let Some(owner) = args.remove("owner") else {
        return Ok(None);
    };
    let Value::String(owner) = owner else {
        return Err("owner must be a string: the email of someone who shared with you".into());
    };
    let owner = owner.trim();
    let lower = owner.to_lowercase();
    if owner == caller.0 || email.is_some_and(|e| e.0 == lower) {
        return Ok(None);
    }
    let scope = match scope(tool) {
        None => return Ok(None),
        Some(Scope::Own) => return Err(format!("{tool} only works on your own account; leave out owner").into()),
        Some(s) => s,
    };
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT owner_sub, gym, vitals FROM share.access
         WHERE grantee_sub = $1 AND (owner_email = $2 OR owner_sub = $3)",
    )
    .bind(&caller.0)
    .bind(&lower)
    .bind(owner)
    .fetch_optional(&db.app)
    .await
    .map_err(db_err)?;
    // Same answer for an unknown user and one who shares nothing with the caller.
    let Some((owner_sub, gym, vitals)) = row else {
        return Err(format!("{owner} has not shared data with you (share_list shows who has)").into());
    };
    let (gym, vitals) = (Access::parse(&gym), Access::parse(&vitals));
    if let Scope::Data(domain, write) = scope {
        let has = match domain {
            Domain::Gym => gym,
            Domain::Vitals => vitals,
        };
        match (has, write) {
            (Access::None, _) => return Err(format!("{owner} has not shared their {domain} data with you").into()),
            (Access::Ro, true) => {
                return Err(format!("{owner} shared their {domain} data read-only with you; {tool} writes").into());
            }
            _ => {}
        }
    }
    Ok(Some(Acting {
        user: User(owner_sub),
        domains: Domains {
            gym: gym != Access::None,
            vitals: vitals != Access::None,
        },
    }))
}

/// A random invite code: 244 bits from two v4 UUIDs (the OS CSPRNG), 64 hex characters.
fn new_code() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// Only this is stored, so a database dump can't accept invites.
fn code_hash(code: &str) -> Vec<u8> {
    Sha256::digest(code.as_bytes()).to_vec()
}

fn verified(parts: &Parts) -> Result<&str, ToolError> {
    parts
        .extensions
        .get::<VerifiedEmail>()
        .map(|e| e.0.as_str())
        .ok_or_else(|| {
            "sharing needs the verified email of your login, and this login has none: log in again \
         (the connector or app must request the `email` scope)"
                .into()
        })
}

/// Trimmed and lowercased; rejects what clearly isn't an address. The mail itself is the real check.
fn parse_email(s: &str) -> Result<String, ToolError> {
    let e = s.trim().to_lowercase();
    let ok = e.len() <= 254
        && !e.chars().any(|c| c.is_whitespace() || c.is_control())
        && e.split_once('@').is_some_and(|(l, d)| !l.is_empty() && d.contains('.'));
    if ok {
        Ok(e)
    } else {
        Err(format!("not an email address: {s:?}").into())
    }
}

/// A pending invite, as the invite page shows it.
#[derive(Debug, sqlx::FromRow)]
pub struct Invite {
    pub id: i64,
    pub owner_sub: String,
    pub owner_email: String,
    pub email: String,
    #[sqlx(try_from = "String")]
    pub gym: Access,
    #[sqlx(try_from = "String")]
    pub vitals: Access,
    pub expires_at: DateTime<Utc>,
}

/// For reading a text column; the CHECK allows only these three.
impl TryFrom<String> for Access {
    type Error = String;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.as_str() {
            "none" | "ro" | "rw" => Ok(Access::parse(&s)),
            _ => Err(format!("bad access level {s:?}")),
        }
    }
}

const INVITE_COLS: &str = "id, owner_sub, owner_email, email, gym, vitals, expires_at";

/// The unexpired invite behind an emailed code.
pub async fn invite_by_code(db: &Db, code: &str) -> sqlx::Result<Option<Invite>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {INVITE_COLS} FROM share.invite WHERE code_hash = $1 AND expires_at > now()"
    )))
    .bind(code_hash(code))
    .fetch_optional(&db.app)
    .await
}

/// Declines by code: the code is the secret, so no login is needed. `false` if there was no such invite.
pub async fn decline_code(db: &Db, code: &str) -> sqlx::Result<bool> {
    let n = sqlx::query("DELETE FROM share.invite WHERE code_hash = $1")
        .bind(code_hash(code))
        .execute(&db.app)
        .await?
        .rows_affected();
    Ok(n > 0)
}

/// Accepts or declines invite `id` as `grantee`, whose verified email must be the invited one.
pub async fn respond(db: &Db, id: i64, grantee: &str, email: &str, accept: bool) -> Result<Invite, ToolError> {
    let mut tx = db.app_tx().await.map_err(db_err)?;
    let inv: Option<Invite> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {INVITE_COLS} FROM share.invite WHERE id = $1 AND expires_at > now() FOR UPDATE"
    )))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_err)?;
    let Some(inv) = inv else {
        return Err(format!("no pending invite {id}: it was answered, withdrawn or has expired").into());
    };
    if inv.email != email {
        return Err(format!("this invite is for {}, but you are logged in as {email}", inv.email).into());
    }
    if inv.owner_sub == grantee {
        return Err("this is your own invite".into());
    }
    if accept {
        sqlx::query(
            "INSERT INTO share.access (owner_sub, grantee_sub, owner_email, grantee_email, gym, vitals)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (owner_sub, grantee_sub) DO UPDATE SET owner_email = EXCLUDED.owner_email,
               grantee_email = EXCLUDED.grantee_email, gym = EXCLUDED.gym, vitals = EXCLUDED.vitals, updated_at = now()",
        )
        .bind(&inv.owner_sub)
        .bind(grantee)
        .bind(&inv.owner_email)
        .bind(email)
        .bind(inv.gym.as_str())
        .bind(inv.vitals.as_str())
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
    }
    sqlx::query("DELETE FROM share.invite WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    Ok(inv)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShareInvite {
    /// Email address of the person to share with. They accept with a login that has this verified address.
    pub email: String,
    /// Your gym logbook: none (default), ro (read) or rw (read and write).
    #[serde(default)]
    pub gym: Access,
    /// Your vitals: none (default), ro or rw.
    #[serde(default)]
    pub vitals: Access,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShareUpdate {
    /// Email of someone you share with.
    pub email: String,
    /// New access to your gym logbook; omit to keep it.
    pub gym: Option<Access>,
    /// New access to your vitals; omit to keep it.
    pub vitals: Option<Access>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShareRemove {
    /// Email of someone you share with, who shares with you, or whom you invited.
    pub email: String,
    /// The one thing to remove.
    pub what: Remove,
}

/// What `share_remove` removes: exactly one of these, never the others.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Remove {
    /// Stop sharing your data with them.
    Revoke,
    /// Stop seeing their data.
    Leave,
    /// Cancel your pending invite to them.
    Withdraw,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShareRespond {
    /// Id of a received invite, from share_list.
    pub invite_id: i64,
    /// true to accept, false to decline.
    pub accept: bool,
}

#[tool_router(router = share_router, vis = "pub(crate)")]
impl Health {
    #[tool(
        description = "Invite someone by email to see (ro) or also change (rw) your gym logbook and/or vitals. They get a mail with a link to accept; the invite expires in 7 days. Inviting the same address again replaces the pending invite.",
        annotations(
            title = "Share data",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    pub async fn share_invite(
        &self,
        Parameters(p): Parameters<ShareInvite>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let owner = user(&parts)?;
        let from = verified(&parts)?;
        let email = parse_email(&p.email)?;
        if email == from {
            return Err("that is your own address".into());
        }
        if p.gym == Access::None && p.vitals == Access::None {
            return Err("share at least one of gym or vitals (ro or rw)".into());
        }
        let mut tx = self.db.app_tx().await.map_err(db_err)?;
        let shared: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT FROM share.access WHERE owner_sub = $1 AND grantee_email = $2)")
                .bind(owner)
                .bind(&email)
                .fetch_one(&mut *tx)
                .await
                .map_err(db_err)?;
        if shared {
            return Err(format!("you already share with {email}; use share_update to change what").into());
        }
        let code = new_code();
        let expires_at: DateTime<Utc> = sqlx::query_scalar(
            "INSERT INTO share.invite (code_hash, owner_sub, owner_email, email, gym, vitals)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (owner_sub, email) DO UPDATE SET code_hash = EXCLUDED.code_hash,
               owner_email = EXCLUDED.owner_email, gym = EXCLUDED.gym, vitals = EXCLUDED.vitals,
               created_at = now(), expires_at = EXCLUDED.expires_at
             RETURNING expires_at",
        )
        .bind(code_hash(&code))
        .bind(owner)
        .bind(from)
        .bind(&email)
        .bind(p.gym.as_str())
        .bind(p.vitals.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;
        let link = format!("{}invite/{code}", self.db.share.public_url);
        // shortcut: without SMTP_URL nothing is mailed and the caller passes the link on; set SMTP_URL in production.
        let mailed = match &self.db.share.mailer {
            None => false,
            Some(m) => {
                let body = format!(
                    "{from} wants to share their health data with you:\n\n  Gym logbook: {}\n  Vitals: {}\n\n\
                     Accept or decline: {link}\n\n\
                     Log in with this address ({email}) to accept. The invite expires on {}.\n",
                    p.gym.describe(),
                    p.vitals.describe(),
                    expires_at.format("%Y-%m-%d %H:%M UTC"),
                );
                // Before the commit: an invite whose mail failed is never stored.
                m.send(&email, &format!("{from} shares health data with you"), body)
                    .await
                    .map_err(|e| {
                        tracing::error!("invite mail failed: {e:#}");
                        ToolError::User("could not send the invite mail; try again later".into())
                    })?;
                true
            }
        };
        tx.commit().await.map_err(db_err)?;
        Ok(json!({
            "invited": email, "gym": p.gym.as_str(), "vitals": p.vitals.as_str(),
            "expires_at": expires_at, "mailed": mailed, "link": link,
        })
        .to_string())
    }

    #[tool(
        description = "Who you share with, who shares with you (pass their email as `owner` to other tools), and pending invites sent and received (accept or decline received ones with share_respond).",
        annotations(title = "List shares", read_only_hint = true, open_world_hint = false)
    )]
    pub async fn share_list(&self, Extension(parts): Extension<Parts>) -> ToolResult {
        let me = user(&parts)?;
        let email = parts.extensions.get::<VerifiedEmail>().map(|e| e.0.as_str());
        let mut tx = self.db.app_tx().await.map_err(db_err)?;
        let out: String = sqlx::query_scalar(
            "SELECT jsonb_build_object(
               'your_email', $2::text,
               'shared_by_you', (SELECT coalesce(jsonb_agg(jsonb_build_object('email', grantee_email, 'gym', gym,
                   'vitals', vitals, 'since', created_at) ORDER BY grantee_email), '[]')
                 FROM share.access WHERE owner_sub = $1),
               'shared_with_you', (SELECT coalesce(jsonb_agg(jsonb_build_object('owner', owner_email, 'gym', gym,
                   'vitals', vitals, 'since', created_at) ORDER BY owner_email), '[]')
                 FROM share.access WHERE grantee_sub = $1),
               'invites_sent', (SELECT coalesce(jsonb_agg(jsonb_build_object('email', email, 'gym', gym,
                   'vitals', vitals, 'expires_at', expires_at) ORDER BY email), '[]')
                 FROM share.invite WHERE owner_sub = $1 AND expires_at > now()),
               'invites_received', (SELECT coalesce(jsonb_agg(jsonb_build_object('invite_id', id, 'from', owner_email,
                   'gym', gym, 'vitals', vitals, 'expires_at', expires_at) ORDER BY id), '[]')
                 FROM share.invite WHERE email = $2 AND owner_sub <> $1 AND expires_at > now())
             )::text",
        )
        .bind(me)
        .bind(email)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;
        Ok(out)
    }

    #[tool(
        description = "Change what you share with someone who accepted your invite. Setting both gym and vitals to none stops sharing with them.",
        annotations(
            title = "Change share",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub async fn share_update(
        &self,
        Parameters(p): Parameters<ShareUpdate>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let owner = user(&parts)?;
        let email = parse_email(&p.email)?;
        let mut tx = self.db.app_tx().await.map_err(db_err)?;
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT gym, vitals FROM share.access WHERE owner_sub = $1 AND grantee_email = $2 FOR UPDATE",
        )
        .bind(owner)
        .bind(&email)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;
        let Some((gym, vitals)) = row else {
            return Err(format!("you don't share with {email} (a pending invite changes by inviting again)").into());
        };
        let gym = p.gym.unwrap_or(Access::parse(&gym));
        let vitals = p.vitals.unwrap_or(Access::parse(&vitals));
        let q = if gym == Access::None && vitals == Access::None {
            "DELETE FROM share.access WHERE owner_sub = $1 AND grantee_email = $2"
        } else {
            "UPDATE share.access SET gym = $3, vitals = $4, updated_at = now() WHERE owner_sub = $1 AND grantee_email = $2"
        };
        sqlx::query(q)
            .bind(owner)
            .bind(&email)
            .bind(gym.as_str())
            .bind(vitals.as_str())
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Ok(json!({"email": email, "gym": gym.as_str(), "vitals": vitals.as_str()}).to_string())
    }

    #[tool(
        description = "Remove one thing, chosen by `what`: revoke (stop sharing your data with email), leave (stop seeing the data email shares with you) or withdraw (cancel your pending invite to email). The other directions are kept.",
        annotations(
            title = "Remove share",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    pub async fn share_remove(
        &self,
        Parameters(p): Parameters<ShareRemove>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let me = user(&parts)?;
        let email = parse_email(&p.email)?;
        let (q, none) = match p.what {
            Remove::Revoke => (
                "DELETE FROM share.access WHERE owner_sub = $1 AND grantee_email = $2",
                "you don't share with",
            ),
            Remove::Leave => (
                "DELETE FROM share.access WHERE grantee_sub = $1 AND owner_email = $2",
                "nothing is shared with you by",
            ),
            Remove::Withdraw => (
                "DELETE FROM share.invite WHERE owner_sub = $1 AND email = $2",
                "you have no pending invite to",
            ),
        };
        let mut tx = self.db.app_tx().await.map_err(db_err)?;
        let n = sqlx::query(q)
            .bind(me)
            .bind(&email)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?
            .rows_affected();
        if n == 0 {
            return Err(format!("{none} {email}").into());
        }
        tx.commit().await.map_err(db_err)?;
        Ok(json!({"email": email, "removed": p.what}).to_string())
    }

    #[tool(
        description = "Accept or decline an invite you received (invite_id from share_list). Accepting needs a login with the invited, verified email.",
        annotations(
            title = "Answer invite",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    pub async fn share_respond(
        &self,
        Parameters(p): Parameters<ShareRespond>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let me = user(&parts)?;
        let email = verified(&parts)?;
        let inv = respond(&self.db, p.invite_id, me, email, p.accept).await?;
        Ok(json!({
            "accepted": p.accept, "owner": inv.owner_email, "gym": inv.gym.as_str(), "vitals": inv.vitals.as_str(),
        })
        .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_remove_needs_one_explicit_what() {
        let p: ShareRemove = serde_json::from_value(json!({"email": "a@b.c", "what": "leave"})).unwrap();
        assert_eq!(p.what, Remove::Leave);
        for bad in [json!({"email": "a@b.c"}), json!({"email": "a@b.c", "what": "all"})] {
            assert!(serde_json::from_value::<ShareRemove>(bad.clone()).is_err(), "{bad}");
        }
    }

    #[test]
    fn domains_render_for_the_guc() {
        assert_eq!(Domains::ALL.to_string(), "gym,vitals");
        assert_eq!(
            Domains {
                gym: false,
                vitals: true
            }
            .to_string(),
            "vitals"
        );
        assert_eq!(
            Domains {
                gym: false,
                vitals: false
            }
            .to_string(),
            ""
        );
    }

    #[test]
    fn emails_are_normalised() {
        assert_eq!(parse_email(" Bob@Example.COM ").unwrap(), "bob@example.com");
        for bad in [
            "",
            "bob",
            "@example.com",
            "bob@localhost",
            "bob @example.com",
            "a\n@b.c",
        ] {
            assert!(parse_email(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn every_tool_has_a_scope_and_data_tools_say_whether_they_write() {
        for t in Health::tool_list() {
            let s = scope(&t.name).unwrap_or_else(|| panic!("{} has no scope", t.name));
            if let Scope::Data(..) = s {
                let hint = t.annotations.as_ref().and_then(|a| a.read_only_hint);
                assert!(hint.is_some(), "{} needs read_only_hint: it decides ro vs rw", t.name);
            }
        }
        assert_eq!(scope("set_add"), Some(Scope::Data(Domain::Gym, true)));
        assert_eq!(scope("progress"), Some(Scope::Data(Domain::Gym, false)));
        assert_eq!(scope("reading_add"), Some(Scope::Data(Domain::Vitals, true)));
        assert_eq!(scope("reading_list"), Some(Scope::Data(Domain::Vitals, false)));
        assert_eq!(scope("sql_query"), Some(Scope::Sql));
        assert_eq!(scope("share_invite"), Some(Scope::Own));
        assert_eq!(scope("nope"), None);
    }
}
