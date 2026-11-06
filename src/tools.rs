use std::collections::BTreeMap;

use axum::http::request::Parts;
use chrono::{DateTime, Duration, Utc};
use rmcp::handler::server::tool::{Extension, ToolRouter};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::AssertSqlSafe;
use uuid::Uuid;

use crate::auth::User;
use crate::db::Db;

type ToolResult = Result<String, String>;

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    HeartRate,
    Hrv,
    BloodPressure,
    Body,
}

const KINDS: [Kind; 4] = [Kind::HeartRate, Kind::Hrv, Kind::BloodPressure, Kind::Body];

impl Kind {
    fn table(self) -> &'static str {
        match self {
            Kind::HeartRate => "heart_rate",
            Kind::Hrv => "hrv",
            Kind::BloodPressure => "blood_pressure",
            Kind::Body => "body",
        }
    }
    fn fields(self) -> &'static [&'static str] {
        match self {
            Kind::HeartRate => &["bpm"],
            Kind::Hrv => &["rmssd_ms"],
            Kind::BloodPressure => &["systolic", "diastolic", "pulse"],
            Kind::Body => &["weight_kg", "fat_pct", "spo2_pct", "temp_c"],
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
    /// blood_pressure: systolic mmHg.
    pub systolic: Option<i32>,
    /// blood_pressure: diastolic mmHg.
    pub diastolic: Option<i32>,
    /// blood_pressure: pulse in bpm (optional).
    pub pulse: Option<i32>,
    /// body: weight in kg (convert lb → kg).
    pub weight_kg: Option<f64>,
    /// body: body fat percentage.
    pub fat_pct: Option<f64>,
    /// body: blood oxygen saturation percentage.
    pub spo2_pct: Option<f64>,
    /// body: temperature in °C (convert °F → °C).
    pub temp_c: Option<f64>,
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
                kind.table(),
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
                kind.table(),
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
    /// Only this source: "manual", "app" or "import".
    pub source: Option<String>,
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
    /// Field to aggregate, e.g. "bpm", "systolic", "weight_kg".
    pub field: String,
    /// Bucket width as a Postgres interval, e.g. "1 day", "1 week", "1 hour". Default "1 day".
    pub bucket: Option<String>,
    /// IANA time zone for day/week boundaries, e.g. "Europe/Amsterdam". Default "UTC".
    pub tz: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub source: Option<String>,
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
pub struct Vitals {
    db: Db,
    #[allow(dead_code, reason = "used by the tool_handler macro")]
    tool_router: ToolRouter<Self>,
}

fn user(parts: &Parts) -> Result<&str, String> {
    parts
        .extensions
        .get::<User>()
        .map(|u| u.0.as_str())
        .ok_or_else(|| "unauthenticated".into())
}

/// What a transient database failure (deadlock, serialization failure) returns; the app gets `503` for it.
pub const RETRY: &str = "temporarily unavailable, retry";

/// Database messages (CHECK violations, SQL errors in sql_query) are useful to the model; anything else is internal.
fn db_err(e: sqlx::Error) -> String {
    match e {
        sqlx::Error::Database(d) if d.code().is_some_and(|c| retryable(&c)) => {
            tracing::warn!("transient database error: {d}");
            RETRY.into()
        }
        sqlx::Error::Database(d) => d.message().to_owned(),
        e => {
            tracing::error!("database error: {e}");
            "internal error".into()
        }
    }
}

/// SQLSTATE class 40 (transaction rollback: 40001 serialization, 40P01 deadlock), except 40002, a constraint
/// violation that a retry repeats. 57014 (sql_query's statement timeout) stays the caller's error.
fn retryable(code: &str) -> bool {
    code.starts_with("40") && code != "40002"
}

fn range(from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) -> (DateTime<Utc>, DateTime<Utc>) {
    let to = to.unwrap_or_else(Utc::now);
    (from.unwrap_or(to - Duration::days(30)), to)
}

/// Inserts stages for a session the caller owns: sleep_add just created it, sleep_update matched user and source.
async fn insert_stages(tx: &mut crate::db::Tx, user: &str, id: Uuid, stages: &[SleepStage]) -> Result<(), String> {
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
impl Vitals {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Log 1–1000 manual readings of one kind (heart_rate, hrv, blood_pressure, body). Units: bpm, ms, mmHg, kg, %, °C."
    )]
    pub async fn reading_add(
        &self,
        Parameters(p): Parameters<ReadingAdd>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        if p.readings.len() > MAX_ROWS {
            return Err(format!("at most {MAX_ROWS} readings per call"));
        }
        let mut rows = Vec::with_capacity(p.readings.len());
        for r in &p.readings {
            let mut m = r.values.to_object(p.kind)?;
            m.insert("time".into(), r.time.to_rfc3339().into());
            m.insert("user_id".into(), user.into());
            m.insert("source".into(), "manual".into());
            rows.push(Value::Object(m));
        }
        let t = p.kind.table();
        let mut tx = self.db.app_tx(user).await.map_err(db_err)?;
        let n = sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO vitals_data.{t} SELECT * FROM jsonb_populate_recordset(NULL::vitals_data.{t}, $1)"
        )))
        .bind(Value::Array(rows))
        .execute(&mut *tx)
        .await
        .map_err(db_err)?
        .rows_affected();
        tx.commit().await.map_err(db_err)?;
        Ok(format!("added {n} {t} reading(s)"))
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
        let t = p.kind.table();
        let mut tx = self.db.app_tx(user).await.map_err(db_err)?;
        let n = sqlx::query(AssertSqlSafe(format!(
            "UPDATE vitals_data.{t} SET ({cols}) = (SELECT {cols} FROM jsonb_populate_record(NULL::vitals_data.{t}, $1))
             WHERE user_id = $3 AND time = $2 AND source = 'manual'"
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
            0 => Err(format!("no manual {t} reading at {}", p.time.to_rfc3339())),
            _ => Ok(format!("updated {t} reading at {}", p.time.to_rfc3339())),
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
        let t = p.kind.table();
        let mut tx = self.db.app_tx(user).await.map_err(db_err)?;
        let n = sqlx::query(AssertSqlSafe(format!(
            "DELETE FROM vitals_data.{t}
             WHERE user_id = $4 AND source = 'manual' AND (time = ANY($1) OR (time >= $2 AND time < $3))"
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
        Ok(format!("deleted {n} manual {t} reading(s)"))
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
        let (t, dir) = (p.kind.table(), if p.oldest_first { "ASC" } else { "DESC" });
        let mut tx = self.db.app_tx(user).await.map_err(db_err)?;
        sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT coalesce(json_agg(to_jsonb(r) - 'user_id' ORDER BY r.time {dir}), '[]')::text FROM (
               SELECT * FROM vitals_data.{t}
               WHERE user_id = $5 AND time >= $1 AND time < $2 AND ($3::text IS NULL OR source::text = $3)
               ORDER BY time {dir} LIMIT $4) r"
        )))
        .bind(from)
        .bind(to)
        .bind(p.source)
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
                p.kind.table(),
                p.kind.fields()
            )
        })?;
        let (from, to) = range(p.from, p.to);
        let t = p.kind.table();
        let mut tx = self.db.app_tx(user).await.map_err(db_err)?;
        let rows: Vec<String> = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT row_to_json(b)::text FROM (
               SELECT time_bucket($1::interval, time, $2) AS bucket, count({f}) AS n,
                      round(avg({f})::numeric, 2) AS avg, min({f}) AS min, max({f}) AS max,
                      round(stddev_samp({f})::numeric, 2) AS stddev,
                      percentile_cont(0.5) WITHIN GROUP (ORDER BY {f}) AS p50,
                      percentile_cont(0.9) WITHIN GROUP (ORDER BY {f}) AS p90
               FROM vitals_data.{t}
               WHERE user_id = $6 AND time >= $3 AND time < $4 AND {f} IS NOT NULL
                 AND ($5::text IS NULL OR source::text = $5)
               GROUP BY 1 ORDER BY 1 LIMIT {}) b ORDER BY b.bucket",
            MAX_ROWS + 1
        )))
        .bind(p.bucket.as_deref().unwrap_or("1 day"))
        .bind(p.tz.as_deref().unwrap_or("UTC"))
        .bind(from)
        .bind(to)
        .bind(p.source)
        .bind(user)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;
        Ok(capped_json(&rows, "buckets"))
    }

    #[tool(description = "Log a manual sleep session, optionally with stage segments (awake, light, deep, rem).")]
    pub async fn sleep_add(
        &self,
        Parameters(p): Parameters<SleepAdd>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        let mut tx = self.db.app_tx(user).await.map_err(db_err)?;
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
        Ok(format!("added sleep session {id}"))
    }

    #[tool(description = "Change a manual sleep session; `stages` replaces all its stages.")]
    pub async fn sleep_update(
        &self,
        Parameters(p): Parameters<SleepUpdate>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        let mut tx = self.db.app_tx(user).await.map_err(db_err)?;
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
            return Err(format!("no manual sleep session {}", p.id));
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
        Ok(format!("updated sleep session {}", p.id))
    }

    #[tool(description = "Delete a manual sleep session and its stages.")]
    pub async fn sleep_delete(
        &self,
        Parameters(p): Parameters<SleepDelete>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        let mut tx = self.db.app_tx(user).await.map_err(db_err)?;
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
            0 => Err(format!("no manual sleep session {}", p.id)),
            _ => Ok(format!("deleted sleep session {}", p.id)),
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
        let mut tx = self.db.app_tx(user).await.map_err(db_err)?;
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
            let t = k.table();
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
                "'{t}', (SELECT jsonb_build_object(
                   'latest', (SELECT to_jsonb(r) - 'user_id' FROM vitals_data.{t} r WHERE user_id = $1
                              ORDER BY time DESC LIMIT 1), {stats})
                 FROM vitals_data.{t} WHERE user_id = $1 AND time > now() - interval '30 days')"
            ));
        }
        let sql = format!(
            "SELECT jsonb_build_object({}, 'sleep', (
               SELECT jsonb_build_object(
                 'avg_hours_7d', round((avg(h) FILTER (WHERE start_at > now() - interval '7 days'))::numeric, 2),
                 'avg_hours_30d', round(avg(h)::numeric, 2), 'nights_30d', count(*),
                 'stage_pct_30d', (SELECT jsonb_object_agg(stage, pct) FROM (
                    SELECT stage, round(100.0 * sum(duration_s) / sum(sum(duration_s)) OVER (), 1) AS pct
                    FROM vitals_data.sleep_stage WHERE user_id = $1 AND time > now() - interval '31 days'
                    GROUP BY stage) st))
               FROM (SELECT start_at, extract(epoch FROM end_at - start_at) / 3600 AS h
                     FROM vitals_data.sleep_session WHERE user_id = $1 AND start_at > now() - interval '30 days') s)
             )::text",
            parts_sql.join(", ")
        );
        let mut tx = self.db.app_tx(user).await.map_err(db_err)?;
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
        description = "Run one read-only SQL query (PostgreSQL + TimescaleDB, e.g. time_bucket, percentile_cont) over your own data. 5 s timeout, at most 1000 rows. Call schema_describe first."
    )]
    pub async fn sql_query(
        &self,
        Parameters(p): Parameters<SqlQuery>,
        Extension(parts): Extension<Parts>,
    ) -> ToolResult {
        let user = user(&parts)?;
        let sql = p.sql.trim().trim_end_matches(';');
        let mut tx = self.db.reader_tx(user).await.map_err(db_err)?;
        // Newline before `)` so a trailing `--` comment can't swallow the wrapper.
        // `t.*`, not `t`: a result column named `t` would otherwise shadow the whole row.
        let rows: Vec<String> = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT row_to_json(t.*)::text FROM (\n{sql}\n) t LIMIT {}",
            MAX_ROWS + 1
        )))
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.rollback().await.map_err(db_err)?;
        Ok(capped_json(&rows, "rows"))
    }
}

/// The same tools for the app over `/api`: names, arguments and results exactly as over MCP.
impl Vitals {
    /// Every tool with its input JSON Schema, generated from the same structs as MCP's `tools/list`.
    pub fn tool_list() -> Vec<rmcp::model::Tool> {
        Self::tool_router().list_all()
    }

    /// Runs tool `name` with JSON `args` as `user`; `None` if there is no such tool.
    pub async fn call(&self, name: &str, args: Value, user: User) -> Option<ToolResult> {
        let mut parts = axum::http::Request::new(()).into_parts().0;
        parts.extensions.insert(user);
        let x = Extension(parts);
        // Bad arguments are the caller's error, like a tool error: Some(Err).
        macro_rules! with_args {
            ($tool:ident) => {
                match serde_json::from_value(args) {
                    Ok(a) => self.$tool(Parameters(a), x).await,
                    Err(e) => Err(format!("invalid arguments: {e}")),
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
            _ => return None,
        })
    }
}

/// Most rows a tool returns, so one call can't flood the model (a strap writes heart rate every second).
const MAX_ROWS: usize = 1000;

/// `{"truncated": …, "<key>": [...]}` from JSON rows fetched with `LIMIT MAX_ROWS + 1`.
fn capped_json(rows: &[String], key: &str) -> String {
    let truncated = rows.len() > MAX_ROWS;
    let rows = &rows[..rows.len().min(MAX_ROWS)];
    format!(r#"{{"truncated":{truncated},"{key}":[{}]}}"#, rows.join(","))
}

const SCHEMA_DOC: &str = r#"Views (only your own rows are visible; times are timestamptz, UTC unless you convert):

heart_rate     (time, user_id, source, bpm smallint, note, rr_ms smallint[], device_id)  -- rr_ms: beat-to-beat intervals (chest strap only)
hrv            (time, user_id, source, rmssd_ms real, note)            -- RMSSD in ms
blood_pressure (time, user_id, source, systolic smallint, diastolic smallint, pulse smallint, note)  -- mmHg, bpm
body           (time, user_id, source, weight_kg real, fat_pct real, spo2_pct real, temp_c real, note)  -- any subset non-null
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

source is 'manual' for readings logged through this server, 'app' for trackers sent by the phone app, 'import' for importers.
device_id → devices.id says which tracker recorded a row; NULL means unknown (manual, imported, or older app heart_rate rows), not "manual".

Examples:
-- daily average and max heart rate, Amsterdam days
SELECT time_bucket('1 day', time, 'Europe/Amsterdam') AS day, avg(bpm), max(bpm) FROM heart_rate
WHERE time > now() - interval '30 days' GROUP BY 1 ORDER BY 1;
-- blood pressure readings at or above 140/90
SELECT time, systolic, diastolic FROM blood_pressure WHERE systolic >= 140 OR diastolic >= 90 ORDER BY time DESC;
-- deep sleep minutes per night
SELECT s.start_at::date AS night, sum(g.duration_s) / 60 AS deep_min FROM sleep_session s
JOIN sleep_stage g ON g.session_id = s.id AND g.stage = 'deep' GROUP BY 1 ORDER BY 1;
-- HRV vs previous night's sleep duration
SELECT h.time::date, h.rmssd_ms, extract(epoch FROM s.end_at - s.start_at) / 3600 AS sleep_h
FROM hrv h JOIN sleep_session s ON s.end_at::date = h.time::date;
-- nightly skin temperature (02:00-06:00 local) vs the previous 30 nights
WITH n AS (SELECT (t.time AT TIME ZONE 'Europe/Amsterdam')::date AS night, avg(c) AS c FROM skin_temp t, unnest(t.celsius) c
  WHERE extract(hour FROM t.time AT TIME ZONE 'Europe/Amsterdam') BETWEEN 2 AND 5 GROUP BY 1)
SELECT night, round(c::numeric, 2), round((c - avg(c) OVER (ORDER BY night ROWS BETWEEN 30 PRECEDING AND 1 PRECEDING))::numeric, 2) AS delta
FROM n ORDER BY night;"#;

#[tool_handler]
impl ServerHandler for Vitals {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))).with_instructions(
            "Personal vitals log: heart rate, HRV, blood pressure, body (weight, fat %, SpO2, temperature) and sleep. \
             Use summary for an overview, reading_list/reading_aggregate/sleep_list for typed queries, and \
             schema_describe + sql_query for anything else. Write tools only change readings logged manually \
             (source 'manual'); imported readings are read-only. Convert lb to kg and °F to °C before logging.",
        )
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
    fn only_transaction_rollbacks_are_retryable() {
        assert!(retryable("40001") && retryable("40P01"));
        assert!(!retryable("40002") && !retryable("57014") && !retryable("23514"));
    }
}
