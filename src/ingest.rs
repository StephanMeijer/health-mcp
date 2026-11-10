//! `POST /api/v1/ingest`: batches of device frames as JSONL or CBOR sequences, stored all-or-nothing.
//! `GET`/`POST /api/v1/devices`: the user's devices, whose numeric ids the frames carry.
//! `GET /api/v1/tools`, `POST /api/v1/tools/{name}`: the MCP tools, for the app.

use std::collections::HashSet;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Extension, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::decompression::RequestDecompressionLayer;
use uuid::Uuid;

use crate::auth::{User, VerifiedEmail};
use crate::db::Db;
use crate::tools::{Health, SleepStage, ToolError};

/// Applies to the decompressed body, so it also caps zip bombs.
pub const MAX_BODY: usize = 32 << 20;
pub const MAX_FRAMES: usize = 5000;
/// Oldest frame time accepted: the phone's 7-day outbox plus a day of slack. Rejecting implausible times keeps a
/// buggy client from spreading one batch over thousands of hourly ecg/acc chunks.
pub const MAX_AGE: TimeDelta = TimeDelta::days(8);
/// Newest frame time accepted, for clock skew between phone and server.
pub const MAX_SKEW: TimeDelta = TimeDelta::hours(1);
/// Rows one batch may write in the one-row-per-item tables: ecg and acc samples, and sleep stages. Measured on the dev
/// database (TimescaleDB 2.31, recent and compressed chunks alike): ~33 µs per ecg/acc sample and ~39 µs per sleep
/// stage, so a full batch commits in at most ~8 s, well inside the 30 s statement timeout.
pub const MAX_ROWS_PER_BATCH: usize = 200_000;
/// Tool arguments are small; this also applies after decompression.
pub const MAX_TOOL_BODY: usize = 1 << 20;

/// How `ecg` and `acc` arrays are sent: as values, or as differences from the previous value (the first from 0).
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    #[default]
    Absolute,
    Delta,
}

impl Encoding {
    fn is_absolute(&self) -> bool {
        *self == Encoding::Absolute
    }
}

/// Running sum in place; `None` on overflow.
fn undelta<T: Copy + Default>(v: &mut [T], add: fn(T, T) -> Option<T>) -> Option<()> {
    let mut sum = T::default();
    for x in v {
        sum = add(sum, *x)?;
        *x = sum;
    }
    Some(())
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Frame {
    Hr {
        device_id: i32,
        time: DateTime<Utc>,
        bpm: i16,
        #[serde(default)]
        rr_ms: Vec<i16>,
    },
    Ecg {
        device_id: i32,
        time: DateTime<Utc>,
        sample_rate: i16,
        #[serde(default, skip_serializing_if = "Encoding::is_absolute")]
        encoding: Encoding,
        samples: Vec<i32>,
    },
    Acc {
        device_id: i32,
        time: DateTime<Utc>,
        sample_rate: i16,
        #[serde(default, skip_serializing_if = "Encoding::is_absolute")]
        encoding: Encoding,
        x: Vec<i16>,
        y: Vec<i16>,
        z: Vec<i16>,
    },
    Battery {
        device_id: i32,
        time: DateTime<Utc>,
        level: i16,
    },
    Status {
        device_id: i32,
        time: DateTime<Utc>,
        event: String,
    },
    #[serde(rename = "skin_temp")]
    SkinTemp {
        device_id: i32,
        time: DateTime<Utc>,
        sample_rate: i16,
        celsius: Vec<f32>,
    },
    Sleep {
        device_id: i32,
        start_at: DateTime<Utc>,
        end_at: DateTime<Utc>,
        #[serde(default)]
        stages: Vec<SleepStage>,
    },
    /// Polar Nightly Recharge, one per night.
    Recharge {
        device_id: i32,
        night: NaiveDate,
        status: i16,
        ans_charge: f32,
        hrv_ms: Option<f32>,
        breathing_rate: Option<f32>,
        hr_bpm: Option<i16>,
    },
}

impl Frame {
    /// Rebuilds delta-encoded arrays into absolute values, so everything after decoding sees only those.
    fn absolute(&mut self) -> Result<(), &'static str> {
        match self {
            Frame::Ecg { encoding, samples, .. } if *encoding == Encoding::Delta => {
                undelta(samples, i32::checked_add).ok_or("ecg samples must fit in i32 after delta decoding")?;
                *encoding = Encoding::Absolute;
            }
            Frame::Acc { encoding, x, y, z, .. } if *encoding == Encoding::Delta => {
                [x, y, z]
                    .into_iter()
                    .try_for_each(|v| undelta(v, i16::checked_add))
                    .ok_or("acc values must fit in i16 after delta decoding")?;
                *encoding = Encoding::Absolute;
            }
            _ => {}
        }
        Ok(())
    }

    /// Every timestamp in the frame lies in `now - MAX_AGE ..= now + MAX_SKEW` (a recharge `night` by date, ±1 day).
    fn recent(&self, now: DateTime<Utc>) -> Result<(), &'static str> {
        let (oldest, newest) = (now - MAX_AGE, now + MAX_SKEW);
        let ok = |t: &DateTime<Utc>| (oldest..=newest).contains(t);
        let fits = match self {
            Frame::Hr { time, .. }
            | Frame::Ecg { time, .. }
            | Frame::Acc { time, .. }
            | Frame::Battery { time, .. }
            | Frame::Status { time, .. }
            | Frame::SkinTemp { time, .. } => ok(time),
            Frame::Sleep {
                start_at,
                end_at,
                stages,
                ..
            } => ok(start_at) && ok(end_at) && stages.iter().all(|g| ok(&g.time)),
            // `night` is the user's local date, so allow a day either side for any UTC offset.
            Frame::Recharge { night, .. } => {
                let day = TimeDelta::days(1);
                (oldest.date_naive() - day..=newest.date_naive() + day).contains(night)
            }
        };
        if fits {
            Ok(())
        } else {
            Err("times must be at most 8 days old and at most 1 hour in the future")
        }
    }

    /// Mirrors the table CHECKs, so a bad frame is reported by index instead of as a failed INSERT.
    fn check(&self) -> Result<(), &'static str> {
        let (ok, why) = match self {
            Frame::Hr { bpm, .. } => ((20..=250).contains(bpm), "bpm must be 20..=250"),
            Frame::Ecg { sample_rate, .. } => (*sample_rate > 0, "sample_rate must be > 0"),
            Frame::Acc {
                sample_rate, x, y, z, ..
            } => (
                *sample_rate > 0 && x.len() == y.len() && y.len() == z.len(),
                "sample_rate must be > 0 and x, y, z equally long",
            ),
            Frame::Battery { level, .. } => ((0..=100).contains(level), "level must be 0..=100"),
            Frame::Status { event, .. } => (!event.is_empty(), "event must not be empty"),
            Frame::SkinTemp {
                sample_rate, celsius, ..
            } => (
                *sample_rate > 0 && celsius.iter().all(|c| *c > 0.0 && *c < 50.0),
                "sample_rate must be > 0 and celsius within 0..50",
            ),
            Frame::Sleep {
                start_at,
                end_at,
                stages,
                ..
            } => (
                end_at > start_at
                    && *end_at - *start_at <= TimeDelta::hours(24)
                    && stages.iter().all(|g| g.duration_s > 0),
                "end_at must be after start_at and at most 24 h later, stage duration_s > 0",
            ),
            Frame::Recharge {
                status,
                ans_charge,
                hrv_ms,
                breathing_rate,
                hr_bpm,
                ..
            } => (
                (1..=6).contains(status)
                    && (-10.0..=10.0).contains(ans_charge)
                    && hrv_ms.is_none_or(|v| v > 0.0)
                    && breathing_rate.is_none_or(|v| v > 0.0)
                    && hr_bpm.is_none_or(|v| (20..=250).contains(&v)),
                "status must be 1..=6, ans_charge -10..=10, hrv_ms and breathing_rate > 0, hr_bpm 20..=250",
            ),
        };
        if ok { Ok(()) } else { Err(why) }
    }

    /// Rows this frame becomes in the one-row-per-item tables (see [`MAX_ROWS_PER_BATCH`]).
    fn rows(&self) -> usize {
        match self {
            Frame::Ecg { samples, .. } => samples.len(),
            Frame::Acc { x, .. } => x.len(),
            Frame::Sleep { stages, .. } => stages.len(),
            _ => 0,
        }
    }

    fn device_id(&self) -> i32 {
        match self {
            Frame::Hr { device_id, .. }
            | Frame::Ecg { device_id, .. }
            | Frame::Acc { device_id, .. }
            | Frame::Battery { device_id, .. }
            | Frame::Status { device_id, .. }
            | Frame::SkinTemp { device_id, .. }
            | Frame::Sleep { device_id, .. }
            | Frame::Recharge { device_id, .. } => *device_id,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Frame::Hr { .. } => "hr",
            Frame::Ecg { .. } => "ecg",
            Frame::Acc { .. } => "acc",
            Frame::Battery { .. } => "battery",
            Frame::Status { .. } => "status",
            Frame::SkinTemp { .. } => "skin_temp",
            Frame::Sleep { .. } => "sleep",
            Frame::Recharge { .. } => "recharge",
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum DecodeError {
    UnsupportedType,
    TooMany,
    /// More than [`MAX_ROWS_PER_BATCH`] ecg/acc samples and sleep stages in total.
    TooManyRows,
    /// 0-based index of the frame (JSONL: of the non-empty line).
    Invalid(usize, String),
}

/// `now` bounds the frame times (see [`MAX_AGE`]); the handler passes the current time.
pub fn decode(content_type: &str, body: &[u8], now: DateTime<Utc>) -> Result<Vec<Frame>, DecodeError> {
    let mut frames = Vec::new();
    let mut rows = 0;
    let mut push = |frames: &mut Vec<Frame>, mut f: Frame| {
        if frames.len() == MAX_FRAMES {
            return Err(DecodeError::TooMany);
        }
        f.absolute()
            .and_then(|()| f.recent(now))
            .and_then(|()| f.check())
            .map_err(|e| DecodeError::Invalid(frames.len(), e.into()))?;
        // A frame over the cap on its own can't be fixed by splitting the batch, so it's invalid rather than a 413.
        if f.rows() > MAX_ROWS_PER_BATCH {
            return Err(DecodeError::Invalid(
                frames.len(),
                format!("at most {MAX_ROWS_PER_BATCH} samples or sleep stages per frame"),
            ));
        }
        rows += f.rows();
        if rows > MAX_ROWS_PER_BATCH {
            return Err(DecodeError::TooManyRows);
        }
        frames.push(f);
        Ok(())
    };
    match content_type.split(';').next().unwrap_or("").trim() {
        "application/x-ndjson" => {
            for line in body.split(|b| *b == b'\n').filter(|l| !l.trim_ascii().is_empty()) {
                let f = serde_json::from_slice(line).map_err(|e| DecodeError::Invalid(frames.len(), e.to_string()))?;
                push(&mut frames, f)?;
            }
        }
        "application/cbor-seq" => {
            let mut rest = body;
            while !rest.is_empty() {
                let f =
                    ciborium::from_reader(&mut rest).map_err(|e| DecodeError::Invalid(frames.len(), e.to_string()))?;
                push(&mut frames, f)?;
            }
        }
        _ => return Err(DecodeError::UnsupportedType),
    }
    Ok(frames)
}

macro_rules! ecg_sql {
    () => {
        "INSERT INTO vitals_data.ecg (time, user_id, source, device_id, uv)
         SELECT f.time + (s.i - 1) * interval '1 second' / f.sample_rate, $2, 'app', f.device_id, s.uv
         FROM jsonb_to_recordset($1) AS f(time timestamptz, device_id int, sample_rate smallint, samples int[])
         CROSS JOIN LATERAL unnest(f.samples) WITH ORDINALITY AS s(uv, i)"
    };
}

macro_rules! acc_sql {
    () => {
        "INSERT INTO vitals_data.acc (time, user_id, source, device_id, x, y, z)
         SELECT f.time + (s.i - 1) * interval '1 second' / f.sample_rate, $2, 'app', f.device_id, s.x, s.y, s.z
         FROM jsonb_to_recordset($1)
           AS f(time timestamptz, device_id int, sample_rate smallint, x smallint[], y smallint[], z smallint[])
         CROSS JOIN LATERAL unnest(f.x, f.y, f.z) WITH ORDINALITY AS s(x, y, z, i)"
    };
}

/// Per frame kind: set-based inserts; only the first statement's rows count as inserted frames.
/// `jsonb_to_recordset` turns JSON arrays into Postgres arrays.
const INSERTS: [(&str, &[&str]); 8] = [
    (
        "hr",
        &[
            "INSERT INTO vitals_data.heart_rate (time, user_id, source, device_id, bpm, rr_ms)
            SELECT time, $2, 'app', device_id, bpm, nullif(rr_ms, '{}') FROM jsonb_to_recordset($1)
              AS f(time timestamptz, device_id int, bpm smallint, rr_ms smallint[])
            ON CONFLICT DO NOTHING",
        ],
    ),
    (
        "ecg",
        // One row per sample, sample i (1-based) at time + (i - 1) / sample_rate s. First samples go in their own
        // statement so its row count is the number of new frames: the app role can insert but not read (RETURNING).
        &[
            concat!(ecg_sql!(), " WHERE s.i = 1 ON CONFLICT DO NOTHING"),
            concat!(ecg_sql!(), " WHERE s.i > 1 ON CONFLICT DO NOTHING"),
        ],
    ),
    (
        "acc",
        &[
            concat!(acc_sql!(), " WHERE s.i = 1 ON CONFLICT DO NOTHING"),
            concat!(acc_sql!(), " WHERE s.i > 1 ON CONFLICT DO NOTHING"),
        ],
    ),
    (
        "battery",
        &[
            "INSERT INTO vitals_data.device_battery (time, user_id, source, device_id, level)
                 SELECT time, $2, 'app', device_id, level FROM jsonb_to_recordset($1)
                   AS f(time timestamptz, device_id int, level smallint)
                 ON CONFLICT DO NOTHING",
        ],
    ),
    (
        "status",
        &[
            "INSERT INTO vitals_data.device_event (time, user_id, source, device_id, event)
                SELECT time, $2, 'app', device_id, event FROM jsonb_to_recordset($1)
                  AS f(time timestamptz, device_id int, event text)
                ON CONFLICT DO NOTHING",
        ],
    ),
    (
        "skin_temp",
        &[
            "INSERT INTO vitals_data.skin_temp (time, user_id, source, device_id, sample_rate, celsius)
             SELECT time, $2, 'app', device_id, sample_rate, celsius FROM jsonb_to_recordset($1)
               AS f(time timestamptz, device_id int, sample_rate smallint, celsius real[])
             ON CONFLICT DO NOTHING",
        ],
    ),
    (
        "recharge",
        &["INSERT INTO vitals_data.recharge
               (night, user_id, source, device_id, status, ans_charge, hrv_ms, breathing_rate, hr_bpm)
             SELECT night, $2, 'app', device_id, status, ans_charge, hrv_ms, breathing_rate, hr_bpm
               FROM jsonb_to_recordset($1) AS f(night date, device_id int, status smallint, ans_charge real,
                                                hrv_ms real, breathing_rate real, hr_bpm smallint)
             ON CONFLICT DO NOTHING"],
    ),
    (
        "sleep",
        &[
            "INSERT INTO vitals_data.sleep_session (user_id, source, device_id, start_at, end_at)
               SELECT $2, 'app', device_id, start_at, end_at FROM jsonb_to_recordset($1)
                 AS f(device_id int, start_at timestamptz, end_at timestamptz)
               ON CONFLICT DO NOTHING",
            // store() runs CHANGED_NIGHTS and DELETE_STAGES before this one. Joins on the natural key: a new night gets
            // its stages, a replaced one its new stages; for an identical resend every stage is already there.
            "INSERT INTO vitals_data.sleep_stage (time, user_id, session_id, stage, duration_s)
               SELECT g.time, $2, s.id, g.stage, g.duration_s
               FROM jsonb_to_recordset($1) AS f(device_id int, start_at timestamptz, stages jsonb)
               JOIN vitals_data.sleep_session s
                 ON s.user_id = $2 AND s.source = 'app' AND s.device_id = f.device_id AND s.start_at = f.start_at
               CROSS JOIN LATERAL jsonb_to_recordset(f.stages) AS g(time timestamptz, stage text, duration_s int)
               ON CONFLICT DO NOTHING",
        ],
    ),
];

/// A re-sent night (same natural key) that differs from the stored one replaces it: the tracker may have finished its
/// analysis since (#90). Changed means another end_at or another list of stages; an identical resend, the usual case
/// after a retry, matches nothing and rewrites nothing. A new night has no stages yet, so it matches too and loses
/// nothing. Sets the new end_at and returns the changed sessions, whose stages [`DELETE_STAGES`] then removes.
const CHANGED_NIGHTS: &str = "WITH f AS (
       SELECT * FROM jsonb_to_recordset($1) AS f(device_id int, start_at timestamptz, end_at timestamptz, stages jsonb)),
     changed AS (
       SELECT s.id, f.end_at FROM f
       JOIN vitals_data.sleep_session s
         ON s.user_id = $2 AND s.source = 'app' AND s.device_id = f.device_id AND s.start_at = f.start_at
       WHERE s.end_at IS DISTINCT FROM f.end_at
          OR (SELECT string_agg(format('%s|%s|%s', g.time, g.stage, g.duration_s), ',' ORDER BY g.time)
                FROM vitals_data.sleep_stage g WHERE g.user_id = $2 AND g.session_id = s.id)
             IS DISTINCT FROM
             (SELECT string_agg(format('%s|%s|%s', x.time, x.stage, x.duration_s), ',' ORDER BY x.time)
                FROM jsonb_to_recordset(coalesce(f.stages, '[]')) AS x(time timestamptz, stage text, duration_s int))),
     ended AS (
       UPDATE vitals_data.sleep_session s SET end_at = c.end_at FROM changed c
       WHERE s.id = c.id AND s.end_at IS DISTINCT FROM c.end_at)
     SELECT id FROM changed";

/// Session ids bound as a constant: TimescaleDB then decompresses only those sessions' batches, where a join on
/// `changed` would decompress every compressed stage batch of the user (#123).
const DELETE_STAGES: &str = "DELETE FROM vitals_data.sleep_stage WHERE user_id = $2 AND session_id = ANY($1)";

pub enum StoreError {
    /// Index of the first frame whose `device_id` isn't one of the user's devices.
    UnknownDevice(usize),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for StoreError {
    fn from(e: sqlx::Error) -> Self {
        StoreError::Db(e)
    }
}

/// Inserts all frames in one transaction; returns how many rows were new.
pub async fn store(db: &Db, user: &str, frames: &[Frame]) -> Result<u64, StoreError> {
    let mut tx = db.app_tx().await?;
    // No RLS: a device FK only proves the device exists, so every frame's device must be one of the user's.
    let mut ids: Vec<i32> = frames.iter().map(Frame::device_id).collect();
    ids.sort_unstable();
    ids.dedup();
    let own: Vec<i32> = sqlx::query_scalar("SELECT id FROM vitals_data.devices WHERE id = ANY($1) AND user_id = $2")
        .bind(&ids)
        .bind(user)
        .fetch_all(&mut *tx)
        .await?;
    if let Some(i) = frames.iter().position(|f| !own.contains(&f.device_id())) {
        return Err(StoreError::UnknownDevice(i));
    }
    let mut inserted = 0;
    for (kind, sqls) in INSERTS {
        let mut rows: Vec<&Frame> = frames.iter().filter(|f| f.kind() == kind).collect();
        if rows.is_empty() {
            continue;
        }
        if kind == "sleep" {
            // Two versions of one night in a batch: the last one sent wins, as it would across batches (#90).
            let mut seen = HashSet::new();
            rows.reverse();
            rows.retain(|f| match f {
                Frame::Sleep {
                    device_id, start_at, ..
                } => seen.insert((*device_id, *start_at)),
                _ => true,
            });
            rows.reverse();
        }
        for (i, sql) in sqls.iter().enumerate() {
            if kind == "sleep" && i == 1 {
                let changed: Vec<Uuid> = sqlx::query_scalar(CHANGED_NIGHTS)
                    .bind(sqlx::types::Json(&rows))
                    .bind(user)
                    .fetch_all(&mut *tx)
                    .await?;
                if !changed.is_empty() {
                    sqlx::query(DELETE_STAGES)
                        .bind(&changed)
                        .bind(user)
                        .execute(&mut *tx)
                        .await?;
                }
            }
            let n = sqlx::query(*sql)
                .bind(sqlx::types::Json(&rows))
                .bind(user)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            if i == 0 {
                inserted += n;
            }
        }
    }
    tx.commit().await?;
    Ok(inserted)
}

/// The time ingest checks frame times against, instead of the system clock. Only server-side code can add a request
/// extension, so clients can't set it; tests use it to ingest frames dated around a fixed day.
#[derive(Clone, Copy)]
pub struct Clock(pub DateTime<Utc>);

async fn ingest(
    State(db): State<Db>,
    Extension(User(user)): Extension<User>,
    clock: Option<Extension<Clock>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let now = clock.map_or_else(Utc::now, |Extension(Clock(t))| t);
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let frames = match decode(ct, &body, now) {
        Ok(f) => f,
        Err(DecodeError::UnsupportedType) => {
            return (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "use application/x-ndjson or application/cbor-seq",
            )
                .into_response();
        }
        Err(DecodeError::TooMany) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                format!("at most {MAX_FRAMES} frames per batch"),
            )
                .into_response();
        }
        // The app halves the batch and resends on 413, so an oversized batch no longer fails (and retries) forever.
        Err(DecodeError::TooManyRows) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                format!("at most {MAX_ROWS_PER_BATCH} ecg/acc samples and sleep stages per batch: split it"),
            )
                .into_response();
        }
        Err(DecodeError::Invalid(i, e)) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "frame": i, "error": e }))).into_response();
        }
    };
    match store(&db, &user, &frames).await {
        Ok(inserted) => Json(json!({ "frames": frames.len(), "inserted": inserted })).into_response(),
        Err(StoreError::UnknownDevice(i)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "frame": i, "error": "unknown device_id: register it with POST /api/v1/devices" })),
        )
            .into_response(),
        Err(StoreError::Db(e)) => {
            tracing::error!("ingest of {} frames failed: {e}", frames.len());
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Serialize, sqlx::FromRow)]
struct Device {
    id: i32,
    model: String,
    serial: String,
}

async fn list_devices(State(db): State<Db>, Extension(User(user)): Extension<User>) -> Response {
    let devices = async {
        let mut tx = db.app_tx().await?;
        sqlx::query_as::<_, Device>("SELECT id, model, serial FROM vitals_data.devices WHERE user_id = $1 ORDER BY id")
            .bind(&user)
            .fetch_all(&mut *tx)
            .await
    };
    match devices.await {
        Ok(d) => Json(d).into_response(),
        Err(e) => {
            tracing::error!("listing devices failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewDevice {
    model: String,
    serial: String,
}

/// Idempotent: registering a known (model, serial) returns its existing id.
async fn register_device(
    State(db): State<Db>,
    Extension(User(user)): Extension<User>,
    body: Result<Json<NewDevice>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let d = match body {
        Ok(Json(d)) => d,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": e.body_text() }))).into_response(),
    };
    // Mirrors the table CHECKs (chars, like Postgres length()).
    if [&d.model, &d.serial]
        .iter()
        .any(|s| !(1..=100).contains(&s.chars().count()))
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "model and serial must be 1..=100 characters" })),
        )
            .into_response();
    }
    let id = async {
        let mut tx = db.app_tx().await?;
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO vitals_data.devices (user_id, model, serial) VALUES ($1, $2, $3)
             ON CONFLICT (user_id, model, serial) DO UPDATE SET model = EXCLUDED.model RETURNING id",
        )
        .bind(&user)
        .bind(&d.model)
        .bind(&d.serial)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok::<_, sqlx::Error>(id)
    };
    match id.await {
        Ok(id) => Json(json!({ "id": id })).into_response(),
        Err(e) => {
            tracing::error!("registering a device failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn list_tools() -> Response {
    Json(Health::tool_list()).into_response()
}

/// The JSON body is the tool's arguments (empty means `{}`); the result is the tool's output, as JSON.
async fn call_tool(
    State(db): State<Db>,
    Extension(user): Extension<User>,
    email: Option<Extension<VerifiedEmail>>,
    Path(name): Path<String>,
    body: Bytes,
) -> Response {
    let args = if body.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice(&body) {
            Ok(a) => a,
            Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
        }
    };
    match Health::new(db).call_with(&name, args, user, email.map(|e| e.0)).await {
        None => StatusCode::NOT_FOUND.into_response(),
        // Tools return JSON text; `schema_describe` returns plain text, which becomes a JSON string.
        Some(Ok(out)) => Json(serde_json::from_str(&out).unwrap_or(serde_json::Value::String(out))).into_response(),
        Some(Err(ToolError::Internal)) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        Some(Err(e @ ToolError::Retry)) => {
            (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "error": e.to_string() }))).into_response()
        }
        Some(Err(ToolError::User(e))) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
}

/// The `/api` routes, without auth: callers add a layer that inserts [`User`].
pub fn router<S: Clone + Send + Sync + 'static>(db: Db) -> Router<S> {
    Router::new()
        .route("/v1/devices", get(list_devices).post(register_device))
        .route("/v1/ingest", post(ingest))
        .route("/v1/tools", get(list_tools))
        // Route layers run inside the router's, so this limit replaces MAX_BODY here.
        .route(
            "/v1/tools/{name}",
            post(call_tool).layer(DefaultBodyLimit::max(MAX_TOOL_BODY)),
        )
        .layer(DefaultBodyLimit::max(MAX_BODY))
        // Unknown Content-Encoding → 415, which the app takes as "fall back to gzip".
        .layer(RequestDecompressionLayer::new())
        .with_state(db)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixtures are from 2026-11-04 (a night ending 18:00); decode them as if it were then.
    fn now() -> DateTime<Utc> {
        "2026-11-04T18:00:00Z".parse().unwrap()
    }

    fn dec(content_type: &str, body: &[u8]) -> Result<Vec<Frame>, DecodeError> {
        decode(content_type, body, now())
    }

    fn fixtures() -> Vec<Frame> {
        let t = "2026-11-04T10:00:00.123456Z".parse().unwrap();
        let d = || 1;
        vec![
            Frame::Hr {
                device_id: d(),
                time: t,
                bpm: 62,
                rr_ms: vec![968],
            },
            Frame::Ecg {
                device_id: d(),
                time: t,
                sample_rate: 130,
                encoding: Encoding::Absolute,
                samples: vec![-12, 40, 7],
            },
            Frame::Acc {
                device_id: d(),
                time: t,
                sample_rate: 200,
                encoding: Encoding::Absolute,
                x: vec![1],
                y: vec![2],
                z: vec![-1000],
            },
            Frame::Battery {
                device_id: d(),
                time: t,
                level: 87,
            },
            Frame::Status {
                device_id: d(),
                time: t,
                event: "connected".into(),
            },
            Frame::SkinTemp {
                device_id: d(),
                time: t,
                sample_rate: 1,
                celsius: vec![33.25, 33.5],
            },
            Frame::Sleep {
                device_id: d(),
                start_at: t,
                end_at: t + TimeDelta::hours(8),
                stages: vec![SleepStage {
                    time: t,
                    stage: crate::tools::Stage::Light,
                    duration_s: 600,
                }],
            },
            Frame::Recharge {
                device_id: d(),
                night: "2026-11-04".parse().unwrap(),
                status: 4,
                ans_charge: 2.5,
                hrv_ms: Some(48.0),
                breathing_rate: None,
                hr_bpm: Some(52),
            },
        ]
    }

    fn jsonl(frames: &[Frame]) -> Vec<u8> {
        frames
            .iter()
            .map(|f| serde_json::to_string(f).unwrap() + "\n\n")
            .collect::<String>()
            .into_bytes()
    }

    fn cbor(frames: &[Frame]) -> Vec<u8> {
        let mut out = Vec::new();
        for f in frames {
            ciborium::into_writer(f, &mut out).unwrap();
        }
        out
    }

    #[test]
    fn jsonl_and_cbor_decode_to_the_same_frames() {
        let f = fixtures();
        assert_eq!(dec("application/x-ndjson; charset=utf-8", &jsonl(&f)).unwrap(), f);
        assert_eq!(dec("application/cbor-seq", &cbor(&f)).unwrap(), f);
        assert_eq!(dec("application/x-ndjson", b"").unwrap(), vec![]);
    }

    #[test]
    fn reports_the_bad_frame() {
        let body = b"{\"type\":\"battery\",\"device_id\":1,\"time\":\"2026-11-04T10:00:00Z\",\"level\":5}\n\
                     {\"type\":\"battery\",\"device_id\":1,\"time\":\"2026-11-04T10:00:00Z\",\"level\":500}\n";
        assert!(matches!(
            dec("application/x-ndjson", body),
            Err(DecodeError::Invalid(1, _))
        ));
        assert!(matches!(
            dec("application/x-ndjson", b"{\"type\":\"nope\"}"),
            Err(DecodeError::Invalid(0, _))
        ));
        let unknown = br#"{"type":"status","device_id":1,"time":"2026-11-04T10:00:00Z","event":"x","extra":1}"#;
        assert!(matches!(
            dec("application/x-ndjson", unknown),
            Err(DecodeError::Invalid(0, _))
        ));
        let mut truncated = cbor(&fixtures());
        truncated.pop();
        assert!(matches!(
            dec("application/cbor-seq", &truncated),
            Err(DecodeError::Invalid(7, _))
        ));
        for bad in [
            r#"{"type":"skin_temp","device_id":1,"time":"2026-11-04T10:00:00Z","sample_rate":1,"celsius":[33,95]}"#,
            r#"{"type":"sleep","device_id":1,"start_at":"2026-11-04T08:00:00Z","end_at":"2026-11-04T07:00:00Z"}"#,
            r#"{"type":"sleep","device_id":1,"start_at":"2026-11-04T00:00:00Z","end_at":"2026-11-04T07:00:00Z",
                "stages":[{"time":"2026-11-04T00:00:00Z","stage":"n3","duration_s":60}]}"#,
            r#"{"type":"recharge","device_id":1,"night":"2026-11-04","status":7,"ans_charge":0}"#,
            r#"{"type":"ecg","device_id":1,"time":"2026-11-04T10:00:00Z","sample_rate":130,"encoding":"zigzag","samples":[1]}"#,
            r#"{"type":"acc","device_id":1,"time":"2026-11-04T10:00:00Z","sample_rate":200,"encoding":"delta",
                "x":[32000,1000],"y":[0,0],"z":[0,0]}"#,
            r#"{"type":"hr","device_id":1,"time":"2026-11-04T10:00:00Z","bpm":60,"encoding":"delta"}"#,
        ] {
            assert!(
                matches!(
                    dec("application/x-ndjson", bad.replace('\n', "").as_bytes()),
                    Err(DecodeError::Invalid(0, _))
                ),
                "{bad}"
            );
        }
        assert_eq!(dec("application/json", b"[]"), Err(DecodeError::UnsupportedType));
    }

    #[test]
    fn delta_frames_decode_to_absolute() {
        let t: DateTime<Utc> = "2026-11-04T10:00:00Z".parse().unwrap();
        let ecg = |encoding, samples| Frame::Ecg {
            device_id: 1,
            time: t,
            sample_rate: 130,
            encoding,
            samples,
        };
        let acc = |encoding, x, y, z| Frame::Acc {
            device_id: 1,
            time: t,
            sample_rate: 200,
            encoding,
            x,
            y,
            z,
        };
        let sent = [
            ecg(Encoding::Delta, vec![-12, 52, -33]),
            ecg(Encoding::Delta, vec![]),
            acc(Encoding::Delta, vec![1, 2], vec![-1000, 0], vec![32000, -32000]),
        ];
        let stored = vec![
            ecg(Encoding::Absolute, vec![-12, 40, 7]),
            ecg(Encoding::Absolute, vec![]),
            acc(Encoding::Absolute, vec![1, 3], vec![-1000, -1000], vec![32000, 0]),
        ];
        assert_eq!(dec("application/x-ndjson", &jsonl(&sent)).unwrap(), stored);
        assert_eq!(dec("application/cbor-seq", &cbor(&sent)).unwrap(), stored);
        // Absolute frames serialize without the field, so the JSON handed to Postgres is unchanged.
        assert!(!serde_json::to_string(&stored[0]).unwrap().contains("encoding"));
    }

    #[test]
    fn caps_the_batch() {
        let f: Vec<Frame> = (0..=MAX_FRAMES)
            .map(|_| Frame::Status {
                device_id: 1,
                time: now(),
                event: "x".into(),
            })
            .collect();
        assert_eq!(dec("application/x-ndjson", &jsonl(&f)), Err(DecodeError::TooMany));
    }

    #[test]
    fn rejects_frames_outside_the_time_window() {
        let at = |t: DateTime<Utc>| {
            format!(
                r#"{{"type":"battery","device_id":1,"time":"{}","level":5}}"#,
                t.to_rfc3339()
            )
        };
        let sleep = |start: DateTime<Utc>| {
            let end = (start + TimeDelta::minutes(30)).to_rfc3339();
            format!(
                r#"{{"type":"sleep","device_id":1,"start_at":"{}","end_at":"{end}"}}"#,
                start.to_rfc3339()
            )
        };
        let night =
            |d: NaiveDate| format!(r#"{{"type":"recharge","device_id":1,"night":"{d}","status":4,"ans_charge":0}}"#);
        let n = now();
        let ok = [
            at(n - MAX_AGE),
            at(n + MAX_SKEW),
            sleep(n - TimeDelta::hours(8)),
            night((n - MAX_AGE - TimeDelta::days(1)).date_naive()),
            night((n + TimeDelta::days(1)).date_naive()),
        ];
        for f in ok {
            assert!(dec("application/x-ndjson", f.as_bytes()).is_ok(), "{f}");
        }
        let second = TimeDelta::seconds(1);
        let bad = [
            at(n - MAX_AGE - second),
            at(n + MAX_SKEW + second),
            at("2020-01-01T00:00:00Z".parse().unwrap()),
            sleep(n - MAX_AGE - second),
            // ends past the skew allowance
            sleep(n + MAX_SKEW - TimeDelta::minutes(10)),
            night((n - MAX_AGE - TimeDelta::days(2)).date_naive()),
            night((n + TimeDelta::days(2)).date_naive()),
        ];
        for f in bad {
            assert!(
                matches!(
                    dec("application/x-ndjson", f.as_bytes()),
                    Err(DecodeError::Invalid(0, _))
                ),
                "{f}"
            );
        }
    }

    #[test]
    fn caps_samples_per_batch() {
        let t = now();
        let ecg = |n: usize| Frame::Ecg {
            device_id: 1,
            time: t,
            sample_rate: 130,
            encoding: Encoding::Absolute,
            samples: vec![0; n],
        };
        let acc = |n: usize| Frame::Acc {
            device_id: 1,
            time: t,
            sample_rate: 200,
            encoding: Encoding::Absolute,
            x: vec![0; n],
            y: vec![0; n],
            z: vec![0; n],
        };
        // ecg and acc count together, across frames.
        let half = MAX_ROWS_PER_BATCH / 2;
        let at_cap = [ecg(half), acc(MAX_ROWS_PER_BATCH - half)];
        assert_eq!(dec("application/cbor-seq", &cbor(&at_cap)).unwrap().len(), 2);
        let over = [ecg(half), acc(MAX_ROWS_PER_BATCH - half + 1)];
        assert_eq!(dec("application/cbor-seq", &cbor(&over)), Err(DecodeError::TooManyRows));
        // One frame over the cap alone can't be split, so it's invalid instead.
        assert!(matches!(
            dec("application/cbor-seq", &cbor(&[ecg(1), acc(MAX_ROWS_PER_BATCH + 1)])),
            Err(DecodeError::Invalid(1, _))
        ));
        // Sleep stages are rows too (#88): they count with ecg and acc, and alone over the cap they're invalid.
        let sleep = |n: usize| Frame::Sleep {
            device_id: 1,
            start_at: t - TimeDelta::hours(8),
            end_at: t,
            stages: (0..n)
                .map(|i| SleepStage {
                    time: t - TimeDelta::hours(8) + TimeDelta::milliseconds(i as i64),
                    stage: crate::tools::Stage::Light,
                    duration_s: 1,
                })
                .collect(),
        };
        let at_cap = [ecg(half), sleep(MAX_ROWS_PER_BATCH - half)];
        assert_eq!(dec("application/cbor-seq", &cbor(&at_cap)).unwrap().len(), 2);
        let over = [ecg(half), sleep(MAX_ROWS_PER_BATCH - half + 1)];
        assert_eq!(dec("application/cbor-seq", &cbor(&over)), Err(DecodeError::TooManyRows));
        assert!(matches!(
            dec("application/cbor-seq", &cbor(&[sleep(MAX_ROWS_PER_BATCH + 1)])),
            Err(DecodeError::Invalid(0, _))
        ));
    }
}
