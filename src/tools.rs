use std::collections::BTreeMap;

use axum::http::request::Parts;
use chrono::{DateTime, Duration, Utc};
use rmcp::handler::server::tool::{Extension, ToolRouter};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ContentBlock, Implementation, IntoContents, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::{AssertSqlSafe, Executor, SqlSafeStr};
use uuid::Uuid;

use crate::auth::User;
use crate::db::Db;

pub type ToolResult = Result<String, ToolError>;

/// Why a tool call failed. The text of every variant goes to the model; `/api` maps the variant to a status.
#[derive(Debug, PartialEq)]
pub enum ToolError {
    /// The caller's mistake: bad arguments, a CHECK violation, a SQL error in sql_query. The app gets `400`.
    User(String),
    /// Transient database failure (deadlock, restart, overload): nothing ran, trying again is safe. The app gets `503`.
    Retry,
    /// Any other server-side failure, logged here, never detailed to the caller. The app gets `500`.
    Internal,
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::User(m) => f.write_str(m),
            ToolError::Retry => f.write_str("temporarily unavailable, retry"),
            ToolError::Internal => f.write_str("internal error"),
        }
    }
}

impl From<String> for ToolError {
    fn from(m: String) -> Self {
        ToolError::User(m)
    }
}

impl From<&str> for ToolError {
    fn from(m: &str) -> Self {
        ToolError::User(m.to_owned())
    }
}

impl IntoContents for ToolError {
    fn into_contents(self) -> Vec<ContentBlock> {
        vec![ContentBlock::text(self.to_string())]
    }
}

/// What a reading is: heart_rate and hrv have their own tables, every other kind is a `measurement` type
/// (migration 0008) with one `value` in a fixed unit.
#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    HeartRate,
    Hrv,
    Weight,
    BodyFat,
    Height,
    Waist,
    Hip,
    Neck,
    MuscleMass,
    BoneMass,
    BodyWater,
    VisceralFat,
    Spo2,
    Temperature,
    Glucose,
    Systolic,
    Diastolic,
    Pulse,
}

const KINDS: [Kind; 18] = [
    Kind::HeartRate,
    Kind::Hrv,
    Kind::Weight,
    Kind::BodyFat,
    Kind::Height,
    Kind::Waist,
    Kind::Hip,
    Kind::Neck,
    Kind::MuscleMass,
    Kind::BoneMass,
    Kind::BodyWater,
    Kind::VisceralFat,
    Kind::Spo2,
    Kind::Temperature,
    Kind::Glucose,
    Kind::Systolic,
    Kind::Diastolic,
    Kind::Pulse,
];

impl Kind {
    /// The kind's name as callers write it; for measurement kinds also the `measurement.type`.
    fn name(self) -> &'static str {
        match self {
            Kind::HeartRate => "heart_rate",
            Kind::Hrv => "hrv",
            Kind::Weight => "weight",
            Kind::BodyFat => "body_fat",
            Kind::Height => "height",
            Kind::Waist => "waist",
            Kind::Hip => "hip",
            Kind::Neck => "neck",
            Kind::MuscleMass => "muscle_mass",
            Kind::BoneMass => "bone_mass",
            Kind::BodyWater => "body_water",
            Kind::VisceralFat => "visceral_fat",
            Kind::Spo2 => "spo2",
            Kind::Temperature => "temperature",
            Kind::Glucose => "glucose",
            Kind::Systolic => "systolic",
            Kind::Diastolic => "diastolic",
            Kind::Pulse => "pulse",
        }
    }
    /// The unit of a measurement kind, as in `measurement_type`; `None` for kinds with their own table.
    fn unit(self) -> Option<&'static str> {
        match self {
            Kind::HeartRate | Kind::Hrv => None,
            Kind::Weight | Kind::MuscleMass | Kind::BoneMass => Some("kg"),
            Kind::BodyFat | Kind::BodyWater | Kind::Spo2 => Some("%"),
            Kind::Height | Kind::Waist | Kind::Hip | Kind::Neck => Some("cm"),
            Kind::VisceralFat => Some("level"),
            Kind::Temperature => Some("°C"),
            Kind::Glucose => Some("mmol/L"),
            Kind::Systolic | Kind::Diastolic => Some("mmHg"),
            Kind::Pulse => Some("bpm"),
        }
    }
    fn table(self) -> &'static str {
        match self.unit() {
            Some(_) => "measurement",
            None => self.name(),
        }
    }
    fn fields(self) -> &'static [&'static str] {
        match self {
            Kind::HeartRate => &["bpm"],
            Kind::Hrv => &["rmssd_ms"],
            _ => &["value"],
        }
    }
    /// SQL appended to a WHERE on `table()`: measurement kinds share one table. The name is a fixed string.
    fn filter(self) -> String {
        match self.unit() {
            Some(_) => format!(" AND type = '{}'", self.name()),
            None => String::new(),
        }
    }
}

/// Where a reading came from: the `vitals.source` enum (migration 0003).
#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Manual,
    App,
    Import,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Source::Manual => "manual",
            Source::App => "app",
            Source::Import => "import",
        }
    }
}

/// Values of one reading. Only the fields of the chosen `kind` are allowed.
#[derive(Clone, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
pub struct Values {
    /// heart_rate: beats per minute.
    pub bpm: Option<i32>,
    /// hrv: RMSSD in milliseconds.
    pub rmssd_ms: Option<f64>,
    /// Every kind except heart_rate and hrv: the value in the kind's unit (convert lb → kg, in → cm, °F → °C,
    /// mg/dL → mmol/L by dividing by 18).
    pub value: Option<f64>,
    /// Free-text note.
    pub note: Option<String>,
    /// Anything else the caller sent. Rejected in `to_object`: serde would otherwise drop a misspelled
    /// field silently (`deny_unknown_fields` does not work together with `flatten`).
    #[serde(flatten, skip_serializing)]
    #[schemars(skip)]
    pub unknown: BTreeMap<String, Value>,
}

impl Values {
    /// Non-null fields as a JSON object; errors on fields that don't belong to `kind`.
    fn to_object(&self, kind: Kind) -> Result<Map<String, Value>, String> {
        if !self.unknown.is_empty() {
            return Err(format!(
                "unknown field(s) {:?} (allowed for kind '{}': {:?})",
                self.unknown.keys().collect::<Vec<_>>(),
                kind.name(),
                kind.fields()
            ));
        }
        let Value::Object(mut m) = serde_json::to_value(self).expect("plain struct") else {
            unreachable!()
        };
        m.retain(|_, v| !v.is_null());
        if let Some(bad) = m.keys().find(|k| *k != "note" && !kind.fields().contains(&k.as_str())) {
            return Err(format!(
                "field '{bad}' does not belong to kind '{}' (allowed: {:?})",
                kind.name(),
                kind.fields()
            ));
        }
        Ok(m)
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Reading {
    /// When it was measured (RFC 3339).
    pub time: DateTime<Utc>,
    #[serde(flatten)]
    pub values: Values,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadingAdd {
    pub kind: Kind,
    pub readings: Vec<Reading>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadingUpdate {
    pub kind: Kind,
    /// Exact time of the manual reading to change.
    pub time: DateTime<Utc>,
    /// Fields to overwrite.
    pub set: Values,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadingDelete {
    pub kind: Kind,
    /// Exact times of manual readings to delete.
    #[serde(default)]
    pub times: Vec<DateTime<Utc>>,
    /// Or delete all manual readings in [from, to).
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadingList {
    pub kind: Kind,
    /// Default: 30 days before `to`.
    pub from: Option<DateTime<Utc>>,
    /// Default: now.
    pub to: Option<DateTime<Utc>>,
    /// Only this source.
    pub source: Option<Source>,
    /// Max rows, default 100, at most 1000.
    pub limit: Option<i64>,
    /// Newest first (default) or oldest first.
    #[serde(default)]
    pub oldest_first: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadingAggregate {
    pub kind: Kind,
    /// Field to aggregate: "bpm" for heart_rate, "rmssd_ms" for hrv, "value" for every other kind.
    pub field: String,
    /// Bucket width as a Postgres interval, e.g. "1 day", "1 week", "1 hour". Default "1 day".
    pub bucket: Option<String>,
    /// IANA time zone for day/week boundaries, e.g. "Europe/Amsterdam". Default "UTC".
    pub tz: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    /// Only this source.
    pub source: Option<Source>,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Awake,
    Light,
    Deep,
    Rem,
}

#[derive(Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SleepStage {
    /// Start of this stage segment.
    pub time: DateTime<Utc>,
    pub stage: Stage,
    pub duration_s: i32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SleepAdd {
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub note: Option<String>,
    #[serde(default)]
    pub stages: Vec<SleepStage>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SleepUpdate {
    /// Id of a manual sleep session.
    pub id: Uuid,
    pub start_at: Option<DateTime<Utc>>,
    pub end_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
    /// If given, replaces all stages of the session.
    pub stages: Option<Vec<SleepStage>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SleepDelete {
    /// Id of a manual sleep session.
    pub id: Uuid,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SleepList {
    /// Sessions starting in [from, to). Default: last 30 days.
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    /// Include every stage segment, not just per-stage totals.
    #[serde(default)]
    pub include_stages: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SqlQuery {
    /// One read-only SELECT (or WITH … SELECT) over the views listed by schema_describe.
    pub sql: String,
}

#[derive(Clone)]
pub struct Health {
    db: Db,
    /// The gym tools' database handle (src/gym/tools.rs).
    pub(crate) gym: crate::gym::db::App,
    #[allow(dead_code, reason = "used by the tool_handler macro")]
    tool_router: ToolRouter<Self>,
}

pub(crate) fn user(parts: &Parts) -> Result<&str, ToolError> {
    parts
        .extensions
        .get::<User>()
        .map(|u| u.0.as_str())
        .ok_or_else(|| "unauthenticated".into())
}

/// Database messages (CHECK violations, SQL errors in sql_query) are useful to the model; anything else is internal.
fn db_err(e: sqlx::Error) -> ToolError {
    match e {
        sqlx::Error::Database(d) if d.code().is_some_and(|c| retryable(&c)) => {
            tracing::warn!("transient database error: {d}");
            ToolError::Retry
        }
        sqlx::Error::Database(d) => ToolError::User(d.message().to_owned()),
        // Every connection busy: nothing ran, so trying again is safe.
        sqlx::Error::PoolTimedOut => {
            tracing::warn!("database pool timed out");
            ToolError::Retry
        }
        e => {
            tracing::error!("database error: {e}");
            ToolError::Internal
        }
    }
}

/// SQLSTATE class 40 (transaction rollback: 40001 serialization, 40P01 deadlock) except 40002, a constraint
/// violation that a retry repeats; connection lost (08000, 08003, 08006); too many connections (53300) or disk full
/// (53100); and 57P01–57P03 (server shutting down or starting up, as in a restart or failover). Not the rest of 08
/// and 53: a protocol violation (08P01), out of memory (53200) or a temp file limit (53400) fails the same way again.
/// 57014 (sql_query's statement timeout) stays the caller's error.
pub(crate) fn retryable(code: &str) -> bool {
    (code.starts_with("40") && code != "40002")
        || matches!(
            code,
            "08000" | "08003" | "08006" | "53300" | "53100" | "57P01" | "57P02" | "57P03"
        )
}

fn range(from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) -> (DateTime<Utc>, DateTime<Utc>) {
    let to = to.unwrap_or_else(Utc::now);
    (from.unwrap_or(to - Duration::days(30)), to)
}

/// Inserts stages for a session the caller owns: sleep_add just created it, sleep_update matched user and source.
async fn insert_stages(tx: &mut crate::db::Tx, user: &str, id: Uuid, stages: &[SleepStage]) -> Result<(), ToolError> {
    sqlx::query(
        "INSERT INTO vitals_data.sleep_stage (time, user_id, session_id, stage, duration_s)
         SELECT time, $1, $2, stage, duration_s
         FROM jsonb_to_recordset($3) AS s(time timestamptz, stage text, duration_s int)",
    )
    .bind(user)
    .bind(id)
    .bind(serde_json::to_value(stages).expect("plain struct"))
    .execute(&mut **tx)
    .await
    .map_err(db_err)?;
    Ok(())
}

#[tool_router]
impl Health {
    pub fn new(db: Db) -> Self {
        Self {
            gym: db.gym_app(),
            db,
            tool_router: Self::all_tools(),
        }
    }

    #[tool(
        description = "Log 1–1000 manual readings of one kind. heart_rate takes `bpm`, hrv `rmssd_ms`; every other kind takes `value` in a fixed unit: weight, muscle_mass, bone_mass kg; body_fat, body_water, spo2 %; height, waist, hip, neck cm; visceral_fat level; temperature °C; glucose mmol/L; systolic, diastolic mmHg; pulse bpm. Blood pressure is two readings (systolic, diastolic) at the same time, plus pulse if measured."
    )]
    pub async fn reading_add(
        &self,
        Parameters(p): Parameters<ReadingAdd>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        if p.readings.len() > MAX_ROWS {
            return Err(format!("at most {MAX_ROWS} readings per call").into());
        }
        let mut rows = Vec::with_capacity(p.readings.len());
        for r in &p.readings {
            let mut m = r.values.to_object(p.kind)?;
            m.insert("time".into(), r.time.to_rfc3339().into());
            m.insert("user_id".into(), user.into());
            m.insert("source".into(), "manual".into());
            if let Some(unit) = p.kind.unit() {
                m.insert("type".into(), p.kind.name().into());
                m.insert("unit".into(), unit.into());
            }
            rows.push(Value::Object(m));
        }
        let t = p.kind.table();
        let mut tx = self.db.app_tx().await.map_err(db_err)?;
        let n = sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO vitals_data.{t} SELECT * FROM jsonb_populate_recordset(NULL::vitals_data.{t}, $1)"
        )))
        .bind(Value::Array(rows))
        .execute(&mut *tx)
        .await
        .map_err(db_err)?
        .rows_affected();
        tx.commit().await.map_err(db_err)?;
        Ok(json!({"added": n, "kind": p.kind.name()}).to_string())
    }

    #[tool(
        description = "Change fields of one manual reading, identified by kind and exact time. Imported readings cannot be changed."
    )]
    pub async fn reading_update(
        &self,
        Parameters(p): Parameters<ReadingUpdate>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        let set = p.set.to_object(p.kind)?;
        if set.is_empty() {
            return Err("nothing to update".into());
        }
        // Column names come from Kind::fields (validated above), never from free text.
        let cols = set.keys().cloned().collect::<Vec<_>>().join(", ");
        let (t, filter) = (p.kind.table(), p.kind.filter());
        let mut tx = self.db.app_tx().await.map_err(db_err)?;
        let n = sqlx::query(AssertSqlSafe(format!(
            "UPDATE vitals_data.{t} SET ({cols}) = (SELECT {cols} FROM jsonb_populate_record(NULL::vitals_data.{t}, $1))
             WHERE user_id = $3 AND time = $2 AND source = 'manual'{filter}"
        )))
        .bind(Value::Object(set))
        .bind(p.time)
        .bind(user)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?
        .rows_affected();
        tx.commit().await.map_err(db_err)?;
        match n {
            0 => Err(format!("no manual {} reading at {}", p.kind.name(), p.time.to_rfc3339()).into()),
            _ => Ok(json!({"updated": 1, "kind": p.kind.name(), "time": p.time}).to_string()),
        }
    }

    #[tool(
        description = "Delete manual readings of one kind, by exact times or by a [from, to) range. Imported readings are never deleted."
    )]
    pub async fn reading_delete(
        &self,
        Parameters(p): Parameters<ReadingDelete>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        let range = match (p.from, p.to) {
            (Some(f), Some(t)) => Some((f, t)),
            (None, None) if !p.times.is_empty() => None,
            _ => return Err("give either `times`, or both `from` and `to`".into()),
        };
        let (t, filter) = (p.kind.table(), p.kind.filter());
        let mut tx = self.db.app_tx().await.map_err(db_err)?;
        let n = sqlx::query(AssertSqlSafe(format!(
            "DELETE FROM vitals_data.{t}
             WHERE user_id = $4 AND source = 'manual' AND (time = ANY($1) OR (time >= $2 AND time < $3)){filter}"
        )))
        .bind(&p.times)
        .bind(range.map(|r| r.0))
        .bind(range.map(|r| r.1))
        .bind(user)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?
        .rows_affected();
        tx.commit().await.map_err(db_err)?;
        Ok(json!({"deleted": n, "kind": p.kind.name()}).to_string())
    }

    #[tool(description = "List raw readings of one kind in a time range (default: last 30 days), newest first.")]
    pub async fn reading_list(
        &self,
        Parameters(p): Parameters<ReadingList>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        let (from, to) = range(p.from, p.to);
        let limit = p.limit.unwrap_or(100).clamp(1, 1000);
        let (t, filter, dir) = (
            p.kind.table(),
            p.kind.filter(),
            if p.oldest_first { "ASC" } else { "DESC" },
        );
        let mut tx = self.db.app_tx().await.map_err(db_err)?;
        sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT coalesce(json_agg(to_jsonb(r) - 'user_id' ORDER BY r.time {dir}), '[]')::text FROM (
               SELECT * FROM vitals_data.{t}
               WHERE user_id = $5 AND time >= $1 AND time < $2 AND ($3::text IS NULL OR source::text = $3){filter}
               ORDER BY time {dir} LIMIT $4) r"
        )))
        .bind(from)
        .bind(to)
        .bind(p.source.map(Source::as_str))
        .bind(limit)
        .bind(user)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)
    }

    #[tool(
        description = "Aggregate one field per time bucket: n, avg, min, max, stddev, p50, p90. E.g. daily resting heart rate, weekly weight trend. At most 1000 buckets; truncated = true means use a wider bucket or a shorter range."
    )]
    pub async fn reading_aggregate(
        &self,
        Parameters(p): Parameters<ReadingAggregate>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        let f = p.kind.fields().iter().find(|f| **f == p.field).ok_or_else(|| {
            format!(
                "field '{}' does not belong to kind '{}' (allowed: {:?})",
                p.field,
                p.kind.name(),
                p.kind.fields()
            )
        })?;
        let (from, to) = range(p.from, p.to);
        let (t, filter) = (p.kind.table(), p.kind.filter());
        let mut tx = self.db.app_tx().await.map_err(db_err)?;
        let rows: Vec<String> = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT row_to_json(b)::text FROM (
               SELECT time_bucket($1::interval, time, $2) AS bucket, count({f}) AS n,
                      round(avg({f})::numeric, 2) AS avg, min({f}) AS min, max({f}) AS max,
                      round(stddev_samp({f})::numeric, 2) AS stddev,
                      percentile_cont(0.5) WITHIN GROUP (ORDER BY {f}) AS p50,
                      percentile_cont(0.9) WITHIN GROUP (ORDER BY {f}) AS p90
               FROM vitals_data.{t}
               WHERE user_id = $6 AND time >= $3 AND time < $4 AND {f} IS NOT NULL
                 AND ($5::text IS NULL OR source::text = $5){filter}
               GROUP BY 1 ORDER BY 1 LIMIT {}) b ORDER BY b.bucket",
            MAX_ROWS + 1
        )))
        .bind(p.bucket.as_deref().unwrap_or("1 day"))
        .bind(p.tz.as_deref().unwrap_or("UTC"))
        .bind(from)
        .bind(to)
        .bind(p.source.map(Source::as_str))
        .bind(user)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;
        Ok(capped_json(&rows, "buckets", false))
    }

    #[tool(description = "Log a manual sleep session, optionally with stage segments (awake, light, deep, rem).")]
    pub async fn sleep_add(
        &self,
        Parameters(p): Parameters<SleepAdd>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        let mut tx = self.db.app_tx().await.map_err(db_err)?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO vitals_data.sleep_session (user_id, source, start_at, end_at, note)
             VALUES ($1, 'manual', $2, $3, $4) RETURNING id",
        )
        .bind(user)
        .bind(p.start_at)
        .bind(p.end_at)
        .bind(p.note)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;
        insert_stages(&mut tx, user, id, &p.stages).await?;
        tx.commit().await.map_err(db_err)?;
        Ok(json!({"id": id}).to_string())
    }

    #[tool(description = "Change a manual sleep session; `stages` replaces all its stages.")]
    pub async fn sleep_update(
        &self,
        Parameters(p): Parameters<SleepUpdate>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        let mut tx = self.db.app_tx().await.map_err(db_err)?;
        let n = sqlx::query(
            "UPDATE vitals_data.sleep_session
             SET start_at = coalesce($2, start_at), end_at = coalesce($3, end_at), note = coalesce($4, note)
             WHERE id = $1 AND user_id = $5 AND source = 'manual'",
        )
        .bind(p.id)
        .bind(p.start_at)
        .bind(p.end_at)
        .bind(p.note)
        .bind(user)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?
        .rows_affected();
        if n == 0 {
            return Err(format!("no manual sleep session {}", p.id).into());
        }
        if let Some(stages) = p.stages {
            sqlx::query("DELETE FROM vitals_data.sleep_stage WHERE session_id = $1 AND user_id = $2")
                .bind(p.id)
                .bind(user)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
            insert_stages(&mut tx, user, p.id, &stages).await?;
        }
        tx.commit().await.map_err(db_err)?;
        Ok(json!({"id": p.id}).to_string())
    }

    #[tool(description = "Delete a manual sleep session and its stages.")]
    pub async fn sleep_delete(
        &self,
        Parameters(p): Parameters<SleepDelete>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        let mut tx = self.db.app_tx().await.map_err(db_err)?;
        let n =
            sqlx::query("DELETE FROM vitals_data.sleep_session WHERE id = $1 AND user_id = $2 AND source = 'manual'")
                .bind(p.id)
                .bind(user)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?
                .rows_affected();
        tx.commit().await.map_err(db_err)?;
        match n {
            0 => Err(format!("no manual sleep session {}", p.id).into()),
            _ => Ok(json!({"id": p.id}).to_string()),
        }
    }

    #[tool(
        description = "List sleep sessions starting in a range (default: last 30 days) with hours and seconds per stage."
    )]
    pub async fn sleep_list(
        &self,
        Parameters(p): Parameters<SleepList>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        let (from, to) = range(p.from, p.to);
        let mut tx = self.db.app_tx().await.map_err(db_err)?;
        sqlx::query_scalar(
            "SELECT coalesce(json_agg(x ORDER BY x.start_at), '[]')::text FROM (
               SELECT s.id, s.source, s.start_at, s.end_at, s.note,
                      round((extract(epoch FROM s.end_at - s.start_at) / 3600)::numeric, 2) AS hours,
                      (SELECT jsonb_object_agg(stage, total) FROM (
                         SELECT stage, sum(duration_s) AS total FROM vitals_data.sleep_stage
                         WHERE session_id = s.id GROUP BY stage) q) AS stage_seconds,
                      CASE WHEN $3 THEN (SELECT json_agg(g ORDER BY g.time) FROM (
                         SELECT time, stage, duration_s FROM vitals_data.sleep_stage WHERE session_id = s.id) g)
                      END AS stages
               FROM vitals_data.sleep_session s
               WHERE s.user_id = $4 AND s.start_at >= $1 AND s.start_at < $2) x",
        )
        .bind(from)
        .bind(to)
        .bind(p.include_stages)
        .bind(user)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)
    }

    #[tool(
        description = "Overview: latest reading of every vital, 7- and 30-day averages, and sleep duration and stage split."
    )]
    pub async fn summary(&self, Extension(parts): Extension<Parts>) -> ToolResult {
        let user = user(&parts)?;
        let mut parts_sql = Vec::new();
        for k in KINDS {
            let (name, t, filter) = (k.name(), k.table(), k.filter());
            let stats = k
                .fields()
                .iter()
                .map(|f| {
                    format!(
                        "'{f}', jsonb_build_object(
                           'avg_7d', round((avg({f}) FILTER (WHERE time > now() - interval '7 days'))::numeric, 1),
                           'avg_30d', round(avg({f})::numeric, 1), 'n_30d', count({f}))"
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            parts_sql.push(format!(
                "'{name}', (SELECT jsonb_build_object(
                   'latest', (SELECT to_jsonb(r) - 'user_id' FROM vitals_data.{t} r WHERE user_id = $1{filter}
                              ORDER BY time DESC LIMIT 1), {stats})
                 FROM vitals_data.{t} WHERE user_id = $1{filter} AND time > now() - interval '30 days')"
            ));
        }
        let sql = format!(
            "SELECT jsonb_build_object({}, 'sleep', (
               SELECT jsonb_build_object(
                 'avg_hours_7d', round((avg(h) FILTER (WHERE start_at > now() - interval '7 days'))::numeric, 2),
                 'avg_hours_30d', round(avg(h)::numeric, 2), 'nights_30d', count(*),
                 'stage_pct_30d', (SELECT jsonb_object_agg(stage, pct) FROM (
                    SELECT stage, round(100.0 * sum(duration_s) / sum(sum(duration_s)) OVER (), 1) AS pct
                    FROM vitals_data.sleep_stage g JOIN vitals_data.sleep_session n ON n.id = g.session_id
                    WHERE g.user_id = $1 AND n.user_id = $1 AND n.start_at > now() - interval '30 days'
                    GROUP BY stage) st))
               FROM (SELECT start_at, extract(epoch FROM end_at - start_at) / 3600 AS h
                     FROM vitals_data.sleep_session WHERE user_id = $1 AND start_at > now() - interval '30 days') s)
             )::text",
            parts_sql.join(", ")
        );
        let mut tx = self.db.app_tx().await.map_err(db_err)?;
        sqlx::query_scalar(AssertSqlSafe(sql))
            .bind(user)
            .fetch_one(&mut *tx)
            .await
            .map_err(db_err)
    }

    #[tool(
        description = "Describe the views available to sql_query: columns, units and example queries. Call before writing SQL."
    )]
    pub async fn schema_describe(&self) -> String {
        SCHEMA_DOC.to_owned()
    }

    #[tool(
        description = "Run one read-only SQL query (PostgreSQL + TimescaleDB, e.g. time_bucket, percentile_cont) over your own data. 5 s timeout, at most 1000 rows and 1 MiB. Call schema_describe first."
    )]
    pub async fn sql_query(
        &self,
        Parameters(p): Parameters<SqlQuery>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        let sql = p.sql.trim().trim_end_matches(';');
        let mut tx = self.db.reader_tx(user).await.map_err(db_err)?;
        // Postgres parses the bare SQL first: one statement that parses alone has balanced parentheses, quotes and
        // comments, so it can't close the wrapper below and bypass its caps.
        (&mut *tx)
            .prepare(AssertSqlSafe(sql.to_owned()).into_sql_str())
            .await
            .map_err(db_err)?;
        // Newline before `)` so a trailing `--` comment can't swallow the wrapper.
        // `t.*`, not `t`: a result column named `t` would otherwise shadow the whole row.
        // The byte cap runs in Postgres, so an oversized result never reaches this process: rows come back while
        // their running size is within MAX_SQL_BYTES, then the first row past it as NULL, the truncation marker.
        let rows: Vec<Option<String>> = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT CASE WHEN total <= {cap} THEN j END FROM (
               SELECT j, n, sum(octet_length(j)) OVER (ORDER BY n) AS total FROM (
                 SELECT j, row_number() OVER () AS n FROM (
                   SELECT row_to_json(t.*)::text AS j FROM (\n{sql}\n) t LIMIT {limit}) l) r) s
             WHERE total - octet_length(j) <= {cap} ORDER BY n LIMIT {limit}",
            cap = MAX_SQL_BYTES,
            limit = MAX_ROWS + 1
        )))
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.rollback().await.map_err(db_err)?;
        let too_big = rows.last().is_some_and(Option::is_none);
        let rows: Vec<String> = rows.into_iter().flatten().collect();
        Ok(capped_json(&rows, "rows", too_big))
    }
}

/// The same tools for the app over `/api`: names, arguments and results exactly as over MCP.
impl Health {
    /// Every tool with its input JSON Schema, generated from the same structs as MCP's `tools/list`.
    pub fn tool_list() -> Vec<rmcp::model::Tool> {
        Self::all_tools().list_all()
    }

    /// The vitals tools plus the gym tools.
    fn all_tools() -> ToolRouter<Self> {
        Self::tool_router() + Self::gym_router()
    }

    /// Runs tool `name` with JSON `args` as `user`; `None` if there is no such tool.
    pub async fn call(&self, name: &str, args: Value, user: User) -> Option<ToolResult> {
        let owner = user.0.clone();
        let mut parts = axum::http::Request::new(()).into_parts().0;
        parts.extensions.insert(user);
        let x = Extension(parts);
        // Bad arguments are the caller's error, like a tool error: Some(Err).
        macro_rules! with_args {
            ($tool:ident) => {
                match serde_json::from_value(args) {
                    Ok(a) => self.$tool(Parameters(a), x).await,
                    Err(e) => Err(format!("invalid arguments: {e}").into()),
                }
            };
        }
        Some(match name {
            "reading_add" => with_args!(reading_add),
            "reading_update" => with_args!(reading_update),
            "reading_delete" => with_args!(reading_delete),
            "reading_list" => with_args!(reading_list),
            "reading_aggregate" => with_args!(reading_aggregate),
            "sleep_add" => with_args!(sleep_add),
            "sleep_update" => with_args!(sleep_update),
            "sleep_delete" => with_args!(sleep_delete),
            "sleep_list" => with_args!(sleep_list),
            "sql_query" => with_args!(sql_query),
            "summary" => self.summary(x).await,
            "schema_describe" => Ok(self.schema_describe().await),
            _ => {
                return crate::gym::tools::call_api(&self.gym, &owner, name, args)
                    .await
                    .map(crate::gym::tools::out);
            }
        })
    }
}

/// Most rows a tool returns, so one call can't flood the model (a strap writes heart rate every second).
const MAX_ROWS: usize = 1000;

/// Most bytes of row JSON `sql_query` returns (same as the `/api` tool body limit).
const MAX_SQL_BYTES: usize = 1 << 20;

/// `{"truncated": …, "<key>": [...]}` from JSON rows fetched with `LIMIT MAX_ROWS + 1`; `cut` means rows were
/// already dropped for another limit.
fn capped_json(rows: &[String], key: &str, cut: bool) -> String {
    let truncated = cut || rows.len() > MAX_ROWS;
    let rows = &rows[..rows.len().min(MAX_ROWS)];
    format!(r#"{{"truncated":{truncated},"{key}":[{}]}}"#, rows.join(","))
}

const SCHEMA_DOC: &str = r#"Views (only your own rows are visible; times are timestamptz, UTC unless you convert):

heart_rate     (time, user_id, source, bpm smallint, note, rr_ms smallint[], device_id)  -- rr_ms: beat-to-beat intervals (chest strap only)
hrv            (time, user_id, source, rmssd_ms real, note)            -- RMSSD in ms
measurement    (time, user_id, source, device_id, type text, value real, unit text, note)  -- one row per value
measurement_type (name, unit, min, max)  -- every type with its one unit and plausible range:
               -- weight, muscle_mass, bone_mass kg; body_fat, body_water, spo2 %; height, waist, hip, neck cm;
               -- visceral_fat level; temperature °C; glucose mmol/L; systolic, diastolic mmHg; pulse bpm
sleep_session  (id uuid, user_id, source, start_at, end_at, note, device_id)
sleep_stage    (time, user_id, session_id → sleep_session.id, stage 'awake'|'light'|'deep'|'rem', duration_s int)

devices        (id int, user_id, model, serial, created_at)  -- e.g. model 'Polar H10', serial '244E7234'

Raw tracker data (source 'app'):
ecg            (time, user_id, source, device_id, uv int)                                -- one row per sample, µV, 130 Hz
acc            (time, user_id, source, device_id, x smallint, y smallint, z smallint)    -- one row per sample, milli-g, 1000 ≈ 1 g, 200 Hz
device_battery (time, user_id, source, level smallint, device_id)  -- %
device_event   (time, user_id, source, event text, device_id)      -- e.g. connected, disconnected
skin_temp      (time, user_id, source, sample_rate smallint, celsius real[], device_id)  -- wrist skin °C, 1 Hz
recharge       (night date, user_id, source, device_id, status smallint, ans_charge real, hrv_ms real, breathing_rate real, hr_bpm smallint)
               -- Polar Nightly Recharge: status 1 very poor … 6 very good, ans_charge -10..+10 (autonomic recovery vs your baseline),
               -- hrv_ms/breathing_rate/hr_bpm are overnight averages (breaths per minute for breathing_rate)
ecg is ~7.8k rows per minute, acc ~12k: aggregate with time_bucket, never return raw samples for long periods.
Compute HRV from heart_rate.rr_ms (drop intervals < 300 or > 2000 ms), not from bpm. Gaps mean the strap was off or out of range.

Gym logbook (owner_sub = your user id; times in UTC unless you convert; use the gym tools for writes):
user_settings  (owner_sub, timezone, goal, goals_notes, priority_muscle_groups text[], limitations)
gym            (id, owner_sub, name, aliases text[], notes, equipment, created_at)
exercise       (id, owner_sub, name, aliases text[], kind 'strength'|'cardio', muscle_groups text[], secondary_muscle_groups text[], equipment, description)
workout        (id, owner_sub, gym_id → gym.id, started_at, finished_at, finish_reason, time_estimated, notes, created_at)  -- one gym visit
workout_set    (id, workout_id → workout.id, exercise_id → exercise.id, performed_at, time_estimated,
                set_type 'working'|'warmup'|'partial'|'interrupted', reps, weight_kg, unilateral, load_basis, machine, variant,
                rpe, rest_s, duration_s, distance_m, intensity, avg_hr, max_hr, calories, avg_incline_pct, max_incline_pct,
                avg_speed_kmh, max_speed_kmh, notes, logged_at, updated_at)
               -- only set_type 'working' counts as training volume; weight_kg is per dumbbell, barbell total, or machine stack
body_note      (id, owner_sub, noted_at, location, severity 1..10, notes, workout_id)  -- aches, pain, fatigue

source is 'manual' for readings logged through this server, 'app' for trackers sent by the phone app, 'import' for importers.
device_id → devices.id says which tracker recorded a row; NULL means unknown (manual, imported, or older app heart_rate rows), not "manual".

Examples:
-- daily average and max heart rate, Amsterdam days
SELECT time_bucket('1 day', time, 'Europe/Amsterdam') AS day, avg(bpm), max(bpm) FROM heart_rate
WHERE time > now() - interval '30 days' GROUP BY 1 ORDER BY 1;
-- blood pressure readings at or above 140/90 (systolic and diastolic are rows with the same time)
SELECT time, max(value) FILTER (WHERE type = 'systolic') AS sys, max(value) FILTER (WHERE type = 'diastolic') AS dia
FROM measurement WHERE type IN ('systolic', 'diastolic') GROUP BY time
HAVING max(value) FILTER (WHERE type = 'systolic') >= 140 OR max(value) FILTER (WHERE type = 'diastolic') >= 90 ORDER BY time DESC;
-- waist-to-height ratio per day both were measured
SELECT w.time::date, round((w.value / h.value)::numeric, 2) FROM measurement w
JOIN LATERAL (SELECT value FROM measurement WHERE type = 'height' AND time <= w.time ORDER BY time DESC LIMIT 1) h ON true
WHERE w.type = 'waist' ORDER BY 1;
-- deep sleep minutes per night
SELECT s.start_at::date AS night, sum(g.duration_s) / 60 AS deep_min FROM sleep_session s
JOIN sleep_stage g ON g.session_id = s.id AND g.stage = 'deep' GROUP BY 1 ORDER BY 1;
-- HRV vs previous night's sleep duration
SELECT h.time::date, h.rmssd_ms, extract(epoch FROM s.end_at - s.start_at) / 3600 AS sleep_h
FROM hrv h JOIN sleep_session s ON s.end_at::date = h.time::date;
-- working sets per week vs average HRV that week
SELECT w.week, w.sets, round(h.rmssd::numeric, 1) AS avg_rmssd_ms FROM
  (SELECT date_trunc('week', performed_at) AS week, count(*) AS sets FROM workout_set WHERE set_type = 'working' GROUP BY 1) w
  LEFT JOIN (SELECT date_trunc('week', time) AS week, avg(rmssd_ms) AS rmssd FROM hrv GROUP BY 1) h USING (week)
ORDER BY 1;
-- nightly skin temperature (02:00-06:00 local) vs the previous 30 nights
WITH n AS (SELECT (t.time AT TIME ZONE 'Europe/Amsterdam')::date AS night, avg(c) AS c FROM skin_temp t, unnest(t.celsius) c
  WHERE extract(hour FROM t.time AT TIME ZONE 'Europe/Amsterdam') BETWEEN 2 AND 5 GROUP BY 1)
SELECT night, round(c::numeric, 2), round((c - avg(c) OVER (ORDER BY night ROWS BETWEEN 30 PRECEDING AND 1 PRECEDING))::numeric, 2) AS delta
FROM n ORDER BY night;"#;

#[tool_handler]
impl ServerHandler for Health {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))).with_instructions(format!(
            "{}\n\nVitals: personal log of heart rate, HRV, body measurements (weight, body fat, height, waist, blood pressure, \
             glucose, …) and sleep. \
             Use summary for an overview, reading_list/reading_aggregate/sleep_list for typed queries, and \
             schema_describe + sql_query for anything else. Write tools only change readings logged manually \
             (source 'manual'); imported readings are read-only. Convert lb to kg, in to cm, °F to °C and mg/dL to mmol/L before logging. \
             sql_query also sees the gym logbook (workouts, sets, exercises), so questions that span both \
             (training load vs HRV or sleep) are one query.",
            crate::gym::tools::INSTRUCTIONS
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flattened_reading_keeps_time_out_of_unknown() {
        let r: Reading =
            serde_json::from_value(json!({"time": "2026-10-01T07:00:00Z", "bpm": 60, "bpmm": 61})).unwrap();
        assert_eq!(r.values.bpm, Some(60));
        assert_eq!(r.values.unknown.keys().collect::<Vec<_>>(), ["bpmm"]);
    }

    #[test]
    fn transient_errors_are_retryable() {
        for code in ["40001", "40P01", "08006", "53300", "53100", "57P01", "57P03"] {
            assert!(retryable(code), "{code}");
        }
        for code in ["40002", "57014", "23514", "42601", "22P02", "08P01", "53200", "53400"] {
            assert!(!retryable(code), "{code}");
        }
    }
}
