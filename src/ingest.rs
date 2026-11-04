//! `POST /api/v1/ingest`: batches of device frames as JSONL or CBOR sequences, stored all-or-nothing.

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Extension, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::decompression::RequestDecompressionLayer;

use crate::auth::User;
use crate::db::Db;

/// The only source ingest may write (enforced again by the `ingest_insert` RLS policies).
pub const SOURCE: &str = "polar_h10";
/// Applies to the decompressed body, so it also caps zip bombs.
pub const MAX_BODY: usize = 32 << 20;
pub const MAX_FRAMES: usize = 5000;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Frame {
    Hr {
        device_id: String,
        time: DateTime<Utc>,
        bpm: i16,
        #[serde(default)]
        rr_ms: Vec<i16>,
    },
    Ecg {
        device_id: String,
        time: DateTime<Utc>,
        sample_rate: i16,
        samples: Vec<i32>,
    },
    Acc {
        device_id: String,
        time: DateTime<Utc>,
        sample_rate: i16,
        x: Vec<i16>,
        y: Vec<i16>,
        z: Vec<i16>,
    },
    Battery {
        device_id: String,
        time: DateTime<Utc>,
        level: i16,
    },
    Status {
        device_id: String,
        time: DateTime<Utc>,
        event: String,
    },
}

impl Frame {
    /// Mirrors the table CHECKs, so a bad frame is reported by index instead of as a failed INSERT.
    fn check(&self) -> Result<(), &'static str> {
        let (device_id, ok, why) = match self {
            Frame::Hr { device_id, bpm, .. } => (device_id, (20..=250).contains(bpm), "bpm must be 20..=250"),
            Frame::Ecg {
                device_id, sample_rate, ..
            } => (device_id, *sample_rate > 0, "sample_rate must be > 0"),
            Frame::Acc {
                device_id,
                sample_rate,
                x,
                y,
                z,
                ..
            } => (
                device_id,
                *sample_rate > 0 && x.len() == y.len() && y.len() == z.len(),
                "sample_rate must be > 0 and x, y, z equally long",
            ),
            Frame::Battery { device_id, level, .. } => (device_id, (0..=100).contains(level), "level must be 0..=100"),
            Frame::Status { device_id, event, .. } => (device_id, !event.is_empty(), "event must not be empty"),
        };
        if device_id.is_empty() {
            return Err("device_id must not be empty");
        }
        if ok { Ok(()) } else { Err(why) }
    }

    fn kind(&self) -> &'static str {
        match self {
            Frame::Hr { .. } => "hr",
            Frame::Ecg { .. } => "ecg",
            Frame::Acc { .. } => "acc",
            Frame::Battery { .. } => "battery",
            Frame::Status { .. } => "status",
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum DecodeError {
    UnsupportedType,
    TooMany,
    /// 0-based index of the frame (JSONL: of the non-empty line).
    Invalid(usize, String),
}

pub fn decode(content_type: &str, body: &[u8]) -> Result<Vec<Frame>, DecodeError> {
    let mut frames = Vec::new();
    let push = |frames: &mut Vec<Frame>, f: Frame| {
        if frames.len() == MAX_FRAMES {
            return Err(DecodeError::TooMany);
        }
        f.check().map_err(|e| DecodeError::Invalid(frames.len(), e.into()))?;
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

/// Per frame kind: one set-based insert. `jsonb_to_recordset` turns JSON arrays into Postgres arrays.
const INSERTS: [(&str, &str); 5] = [
    (
        "hr",
        "INSERT INTO vitals_data.heart_rate (time, user_id, source, bpm, rr_ms)
            SELECT time, $2, $3, bpm, nullif(rr_ms, '{}') FROM jsonb_to_recordset($1)
              AS f(time timestamptz, bpm smallint, rr_ms smallint[])
            ON CONFLICT DO NOTHING",
    ),
    (
        "ecg",
        "INSERT INTO vitals_data.ecg (time, user_id, source, device_id, sample_rate, samples)
             SELECT time, $2, $3, device_id, sample_rate, samples FROM jsonb_to_recordset($1)
               AS f(time timestamptz, device_id text, sample_rate smallint, samples int[])
             ON CONFLICT DO NOTHING",
    ),
    (
        "acc",
        "INSERT INTO vitals_data.acc (time, user_id, source, device_id, sample_rate, x, y, z)
             SELECT time, $2, $3, device_id, sample_rate, x, y, z FROM jsonb_to_recordset($1)
               AS f(time timestamptz, device_id text, sample_rate smallint, x smallint[], y smallint[], z smallint[])
             ON CONFLICT DO NOTHING",
    ),
    (
        "battery",
        "INSERT INTO vitals_data.device_battery (time, user_id, source, device_id, level)
                 SELECT time, $2, $3, device_id, level FROM jsonb_to_recordset($1)
                   AS f(time timestamptz, device_id text, level smallint)
                 ON CONFLICT DO NOTHING",
    ),
    (
        "status",
        "INSERT INTO vitals_data.device_event (time, user_id, source, device_id, event)
                SELECT time, $2, $3, device_id, event FROM jsonb_to_recordset($1)
                  AS f(time timestamptz, device_id text, event text)
                ON CONFLICT DO NOTHING",
    ),
];

/// Inserts all frames in one transaction; returns how many rows were new.
pub async fn store(db: &Db, user: &str, frames: &[Frame]) -> sqlx::Result<u64> {
    let mut tx = db.app_tx(user).await?;
    let mut inserted = 0;
    for (kind, sql) in INSERTS {
        let rows: Vec<&Frame> = frames.iter().filter(|f| f.kind() == kind).collect();
        if rows.is_empty() {
            continue;
        }
        inserted += sqlx::query(sql)
            .bind(sqlx::types::Json(rows))
            .bind(user)
            .bind(SOURCE)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    }
    tx.commit().await?;
    Ok(inserted)
}

async fn ingest(
    State(db): State<Db>,
    Extension(User(user)): Extension<User>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let frames = match decode(ct, &body) {
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
        Err(DecodeError::Invalid(i, e)) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "frame": i, "error": e }))).into_response();
        }
    };
    match store(&db, &user, &frames).await {
        Ok(inserted) => Json(json!({ "frames": frames.len(), "inserted": inserted })).into_response(),
        Err(e) => {
            tracing::error!("ingest of {} frames failed: {e}", frames.len());
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// The `/api` routes, without auth: callers add a layer that inserts [`User`].
pub fn router<S: Clone + Send + Sync + 'static>(db: Db) -> Router<S> {
    Router::new()
        .route("/v1/ingest", post(ingest))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        // Unknown Content-Encoding → 415, which the app takes as "fall back to gzip".
        .layer(RequestDecompressionLayer::new())
        .with_state(db)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> Vec<Frame> {
        let t = "2026-11-04T10:00:00.123456Z".parse().unwrap();
        let d = || "244E7234".to_owned();
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
                samples: vec![-12, 40, 7],
            },
            Frame::Acc {
                device_id: d(),
                time: t,
                sample_rate: 200,
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
        assert_eq!(decode("application/x-ndjson; charset=utf-8", &jsonl(&f)).unwrap(), f);
        assert_eq!(decode("application/cbor-seq", &cbor(&f)).unwrap(), f);
        assert_eq!(decode("application/x-ndjson", b"").unwrap(), vec![]);
    }

    #[test]
    fn reports_the_bad_frame() {
        let body = b"{\"type\":\"battery\",\"device_id\":\"d\",\"time\":\"2026-11-04T10:00:00Z\",\"level\":5}\n\
                     {\"type\":\"battery\",\"device_id\":\"d\",\"time\":\"2026-11-04T10:00:00Z\",\"level\":500}\n";
        assert!(matches!(
            decode("application/x-ndjson", body),
            Err(DecodeError::Invalid(1, _))
        ));
        assert!(matches!(
            decode("application/x-ndjson", b"{\"type\":\"nope\"}"),
            Err(DecodeError::Invalid(0, _))
        ));
        let unknown = br#"{"type":"status","device_id":"d","time":"2026-11-04T10:00:00Z","event":"x","extra":1}"#;
        assert!(matches!(
            decode("application/x-ndjson", unknown),
            Err(DecodeError::Invalid(0, _))
        ));
        let mut truncated = cbor(&fixtures());
        truncated.pop();
        assert!(matches!(
            decode("application/cbor-seq", &truncated),
            Err(DecodeError::Invalid(4, _))
        ));
        assert_eq!(decode("application/json", b"[]"), Err(DecodeError::UnsupportedType));
    }

    #[test]
    fn caps_the_batch() {
        let f: Vec<Frame> = (0..=MAX_FRAMES)
            .map(|_| Frame::Status {
                device_id: "d".into(),
                time: Utc::now(),
                event: "x".into(),
            })
            .collect();
        assert_eq!(decode("application/x-ndjson", &jsonl(&f)), Err(DecodeError::TooMany));
    }
}
