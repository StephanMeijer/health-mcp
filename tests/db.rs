//! Needs the dev database: `docker compose up -d`. Override URLs with TEST_OWNER_URL / TEST_APP_URL / TEST_READER_URL.

use axum::http::{Request, request::Parts};
use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use serde_json::{Value, json};
use sqlx::PgPool;
use vitals_mcp::auth::User;
use vitals_mcp::db::Db;
use vitals_mcp::tools::*;

fn url(var: &str, default: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| default.into())
}

async fn setup() -> (Vitals, Db, PgPool) {
    let owner = PgPool::connect(&url(
        "TEST_OWNER_URL",
        "postgres://postgres:postgres@localhost:55432/vitals",
    ))
    .await
    .unwrap();
    sqlx::migrate!().run(&owner).await.unwrap();
    let db = Db {
        app: PgPool::connect(&url("TEST_APP_URL", "postgres://vitals_app:app@localhost:55432/vitals"))
            .await
            .unwrap(),
        reader: PgPool::connect(&url(
            "TEST_READER_URL",
            "postgres://vitals_reader:reader@localhost:55432/vitals",
        ))
        .await
        .unwrap(),
    };
    (Vitals::new(db.clone()), db, owner)
}

fn as_user(user: &str) -> Extension<Parts> {
    let (mut parts, _) = Request::new(()).into_parts();
    parts.extensions.insert(User(user.to_owned()));
    Extension(parts)
}

fn uid(name: &str) -> String {
    format!("{name}-{}", uuid::Uuid::new_v4())
}

fn p<T: serde::de::DeserializeOwned>(v: Value) -> Parameters<T> {
    Parameters(serde_json::from_value(v).unwrap())
}

async fn add_hr(v: &Vitals, user: &str, time: &str, bpm: i32) {
    v.reading_add(
        p(json!({"kind": "heart_rate", "readings": [{"time": time, "bpm": bpm}]})),
        as_user(user),
    )
    .await
    .unwrap();
}

async fn sql(v: &Vitals, user: &str, q: &str) -> Result<Value, String> {
    v.sql_query(p(json!({ "sql": q })), as_user(user))
        .await
        .map(|s| serde_json::from_str(&s).unwrap())
}

#[tokio::test]
async fn readings_round_trip() {
    let (v, _, _) = setup().await;
    let a = uid("alice");
    add_hr(&v, &a, "2026-10-01T07:00:00Z", 60).await;
    add_hr(&v, &a, "2026-10-01T19:00:00Z", 70).await;

    let list: Value = serde_json::from_str(
        &v.reading_list(
            p(json!({"kind": "heart_rate", "from": "2026-09-01T00:00:00Z", "to": "2026-11-01T00:00:00Z"})),
            as_user(&a),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert!(list[0].get("user_id").is_none());

    let agg: Value = serde_json::from_str(
        &v.reading_aggregate(
            p(json!({"kind": "heart_rate", "field": "bpm", "tz": "Europe/Amsterdam",
                     "from": "2026-09-01T00:00:00Z", "to": "2026-11-01T00:00:00Z"})),
            as_user(&a),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(agg[0]["n"], 2);
    assert_eq!(agg[0]["avg"], 65.0);

    v.reading_update(
        p(json!({"kind": "heart_rate", "time": "2026-10-01T07:00:00Z", "set": {"bpm": 62}})),
        as_user(&a),
    )
    .await
    .unwrap();
    let rows = sql(&v, &a, "SELECT bpm FROM heart_rate ORDER BY time").await.unwrap();
    assert_eq!(rows["rows"], json!([{"bpm": 62}, {"bpm": 70}]));

    let msg = v
        .reading_delete(
            p(json!({"kind": "heart_rate", "times": ["2026-10-01T19:00:00Z"]})),
            as_user(&a),
        )
        .await
        .unwrap();
    assert_eq!(msg, "deleted 1 manual heart_rate reading(s)");
}

#[tokio::test]
async fn validation_errors_reach_the_model() {
    let (v, _, _) = setup().await;
    let a = uid("alice");
    let too_high = v
        .reading_add(
            p(json!({"kind": "heart_rate", "readings": [{"time": "2026-10-01T07:00:00Z", "bpm": 400}]})),
            as_user(&a),
        )
        .await;
    assert!(too_high.unwrap_err().contains("check constraint"));
    let wrong_field = v
        .reading_add(
            p(json!({"kind": "heart_rate", "readings": [{"time": "2026-10-01T07:00:00Z", "weight_kg": 80.0}]})),
            as_user(&a),
        )
        .await;
    assert!(wrong_field.unwrap_err().contains("does not belong"));
    let bad_agg = v
        .reading_aggregate(p(json!({"kind": "body", "field": "bpm"})), as_user(&a))
        .await;
    assert!(bad_agg.is_err());
}

#[tokio::test]
async fn unknown_fields_are_rejected_not_dropped() {
    let (v, _, _) = setup().await;
    let a = uid("alice");
    let typo = v
        .reading_add(
            p(json!({"kind": "body", "readings": [{"time": "2026-10-01T07:00:00Z", "weight_kg": 80.0, "fatpct": 21.5}]})),
            as_user(&a),
        )
        .await;
    assert!(typo.unwrap_err().contains("fatpct"));
    assert_eq!(sql(&v, &a, "SELECT * FROM body").await.unwrap()["rows"], json!([]));

    v.reading_add(
        p(json!({"kind": "body", "readings": [{"time": "2026-10-01T07:00:00Z", "weight_kg": 80.0}]})),
        as_user(&a),
    )
    .await
    .unwrap();
    let upd = v
        .reading_update(
            p(json!({"kind": "body", "time": "2026-10-01T07:00:00Z", "set": {"weight_kg": 81.0, "weightkg": 82.0}})),
            as_user(&a),
        )
        .await;
    assert!(upd.unwrap_err().contains("weightkg"));
    assert_eq!(
        sql(&v, &a, "SELECT weight_kg FROM body").await.unwrap()["rows"],
        json!([{"weight_kg": 80}])
    );
}

#[tokio::test]
async fn imported_readings_are_read_only() {
    let (v, db, owner) = setup().await;
    let a = uid("alice");
    // Importers bypass RLS; the owner (superuser in dev) stands in for one.
    sqlx::query("INSERT INTO vitals_data.heart_rate VALUES ('2026-10-02T07:00:00Z', $1, 'withings', 55, null)")
        .bind(&a)
        .execute(&owner)
        .await
        .unwrap();

    let upd = v
        .reading_update(
            p(json!({"kind": "heart_rate", "time": "2026-10-02T07:00:00Z", "set": {"bpm": 90}})),
            as_user(&a),
        )
        .await;
    assert!(upd.is_err());
    let del = v
        .reading_delete(
            p(json!({"kind": "heart_rate", "from": "2026-10-01T00:00:00Z", "to": "2026-10-03T00:00:00Z"})),
            as_user(&a),
        )
        .await
        .unwrap();
    assert_eq!(del, "deleted 0 manual heart_rate reading(s)");

    // Even a server bug writing source != 'manual' is stopped by RLS.
    let mut tx = db.app_tx(&a).await.unwrap();
    let raw = sqlx::query("UPDATE vitals_data.heart_rate SET bpm = 90 WHERE source = 'withings'")
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(raw.rows_affected(), 0);
    let ins = sqlx::query("INSERT INTO vitals_data.heart_rate VALUES (now(), $1, 'withings', 60, null)")
        .bind(&a)
        .execute(&mut *tx)
        .await;
    assert!(ins.unwrap_err().to_string().contains("row-level security"));

    let rows = sql(&v, &a, "SELECT bpm, source FROM heart_rate").await.unwrap();
    assert_eq!(rows["rows"], json!([{"bpm": 55, "source": "withings"}]));
}

#[tokio::test]
async fn other_users_rows_are_invisible_and_untouchable() {
    let (v, _, _) = setup().await;
    let (a, b) = (uid("alice"), uid("bob"));
    add_hr(&v, &a, "2026-10-03T07:00:00Z", 61).await;
    add_hr(&v, &b, "2026-10-03T08:00:00Z", 99).await;

    // Bob targets alice's reading; the write tools have no user filter of their own, RLS is what stops this.
    let upd = v
        .reading_update(
            p(json!({"kind": "heart_rate", "time": "2026-10-03T07:00:00Z", "set": {"bpm": 120}})),
            as_user(&b),
        )
        .await;
    assert!(upd.is_err());
    let del = v
        .reading_delete(
            p(json!({"kind": "heart_rate", "times": ["2026-10-03T07:00:00Z"]})),
            as_user(&b),
        )
        .await;
    assert_eq!(del.unwrap(), "deleted 0 manual heart_rate reading(s)");

    assert_eq!(
        sql(&v, &a, "SELECT bpm FROM heart_rate").await.unwrap()["rows"],
        json!([{"bpm": 61}])
    );
    assert_eq!(
        sql(&v, &b, "SELECT bpm FROM heart_rate").await.unwrap()["rows"],
        json!([{"bpm": 99}])
    );
}

#[tokio::test]
async fn sql_query_cannot_escape_the_caller() {
    let (v, db, owner) = setup().await;
    let (a, b) = (uid("alice"), uid("bob"));
    add_hr(&v, &a, "2026-10-04T07:00:00Z", 61).await;
    add_hr(&v, &b, "2026-10-04T07:00:00Z", 99).await;
    sqlx::query("ANALYZE vitals_data.heart_rate")
        .execute(&owner)
        .await
        .unwrap();

    let only_alice = |r: Value| {
        r["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["user_id"] == json!(a))
    };

    assert!(only_alice(sql(&v, &a, "SELECT user_id FROM heart_rate").await.unwrap()));
    let forged = format!("SELECT user_id FROM heart_rate WHERE set_config('vitals.user_id', '{b}', true) IS NOT NULL");
    assert!(sql(&v, &a, &forged).await.map(only_alice).unwrap_or(true));

    let chunk: String = sqlx::query_scalar(
        "SELECT format('%I.%I', chunk_schema, chunk_name) FROM timescaledb_information.chunks WHERE hypertable_name = 'heart_rate' LIMIT 1",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    let stats = sql(
        &v,
        &a,
        "SELECT count(*) AS n FROM pg_stats WHERE schemaname = 'vitals_data'",
    )
    .await
    .unwrap();
    assert_eq!(stats["rows"], json!([{"n": 0}]));

    for attack in [
        format!("SELECT * FROM {chunk}"),
        "SELECT * FROM vitals_data.heart_rate".into(),
        "SELECT set_config('role', 'vitals_app', true)".into(),
        "SELECT vitals_private.sign('x')".into(),
        "SELECT * FROM vitals_private.secret".into(),
        "SELECT 1; SELECT * FROM vitals_data.heart_rate".into(),
        "SELECT 1) t; ROLLBACK; SELECT (1".into(),
        "DELETE FROM heart_rate RETURNING *".into(),
        "WITH d AS (DELETE FROM heart_rate RETURNING *) SELECT * FROM d".into(),
        "SELECT pg_sleep(10)".into(),
    ] {
        assert!(sql(&v, &a, &attack).await.is_err(), "attack succeeded: {attack}");
    }

    // A signature for one user does not work for another.
    let sig: String = sqlx::query_scalar("SELECT vitals_private.sign($1)")
        .bind(&a)
        .fetch_one(&db.app)
        .await
        .unwrap();
    let mut tx = db.reader.begin().await.unwrap();
    sqlx::query("SELECT set_config('vitals.user_id', $1, true), set_config('vitals.user_sig', $2, true)")
        .bind(&b)
        .bind(&sig)
        .execute(&mut *tx)
        .await
        .unwrap();
    let forged_sig = sqlx::query("SELECT * FROM vitals.heart_rate").fetch_all(&mut *tx).await;
    assert!(forged_sig.unwrap_err().to_string().contains("no authenticated user"));
}

#[tokio::test]
async fn sleep_and_summary() {
    let (v, _, _) = setup().await;
    let a = uid("alice");
    let start = chrono::Utc::now() - chrono::Duration::hours(10);
    let t = |m: i64| (start + chrono::Duration::minutes(m)).to_rfc3339();
    v.sleep_add(
        p(json!({"start_at": t(0), "end_at": t(480), "stages": [
            {"time": t(0), "stage": "light", "duration_s": 3600},
            {"time": t(60), "stage": "deep", "duration_s": 1800},
            {"time": t(90), "stage": "rem", "duration_s": 1800}]})),
        as_user(&a),
    )
    .await
    .unwrap();
    add_hr(&v, &a, &t(500), 58).await;

    let nights: Value = serde_json::from_str(&v.sleep_list(p(json!({})), as_user(&a)).await.unwrap()).unwrap();
    assert_eq!(nights[0]["hours"], 8.0);
    assert_eq!(nights[0]["stage_seconds"]["deep"], 1800);

    let s: Value = serde_json::from_str(&v.summary(as_user(&a)).await.unwrap()).unwrap();
    assert_eq!(s["heart_rate"]["latest"]["bpm"], 58);
    assert_eq!(s["sleep"]["nights_30d"], 1);
    assert_eq!(s["sleep"]["stage_pct_30d"]["light"], 50.0);
}
