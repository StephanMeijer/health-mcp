//! Needs the dev database: `docker compose up -d`. Override URLs with TEST_OWNER_URL / TEST_APP_URL / TEST_READER_URL.

use axum::http::{Request, request::Parts};
use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use serde_json::{Value, json};
use sqlx::PgPool;
use vitals_mcp::auth::User;
use vitals_mcp::db::Db;
use vitals_mcp::tools::*;

/// The first sleep_stage rows in a new time range create a TimescaleDB chunk, which copies the foreign key to
/// sleep_session and locks it; two such transactions at once can deadlock (see SPEC.md, Known limits).
/// Tests that write sleep stages run one at a time so a fresh database can't make them flaky.
static SLEEP_WRITES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
    let _serial = SLEEP_WRITES.lock().await;
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

fn session_id(msg: &str) -> String {
    msg.rsplit(' ').next().unwrap().to_owned()
}

async fn stage_seconds(v: &Vitals, user: &str) -> Value {
    let nights: Value = serde_json::from_str(
        &v.sleep_list(
            p(json!({"from": "2026-10-01T00:00:00Z", "to": "2026-11-01T00:00:00Z"})),
            as_user(user),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    nights[0]["stage_seconds"].clone()
}

#[tokio::test]
async fn sleep_round_trip() {
    let _serial = SLEEP_WRITES.lock().await;
    let (v, _, _) = setup().await;
    let (a, b) = (uid("alice"), uid("bob"));
    let added = v
        .sleep_add(
            p(
                json!({"start_at": "2026-10-05T23:00:00Z", "end_at": "2026-10-06T07:00:00Z", "stages": [
                {"time": "2026-10-05T23:00:00Z", "stage": "light", "duration_s": 3600},
                {"time": "2026-10-06T00:00:00Z", "stage": "deep", "duration_s": 1800}]}),
            ),
            as_user(&a),
        )
        .await
        .unwrap();
    let id = session_id(&added);

    // `stages` replaces all of them.
    v.sleep_update(
        p(json!({"id": id, "stages": [{"time": "2026-10-06T01:00:00Z", "stage": "rem", "duration_s": 600}]})),
        as_user(&a),
    )
    .await
    .unwrap();
    assert_eq!(stage_seconds(&v, &a).await, json!({"rem": 600}));

    // Without `stages`, they are kept.
    v.sleep_update(p(json!({"id": id, "note": "woke up twice"})), as_user(&a))
        .await
        .unwrap();
    assert_eq!(stage_seconds(&v, &a).await, json!({"rem": 600}));
    let note = sql(&v, &a, "SELECT note FROM sleep_session").await.unwrap();
    assert_eq!(note["rows"], json!([{"note": "woke up twice"}]));

    // Another user can neither change nor delete it.
    assert!(
        v.sleep_update(p(json!({"id": id, "note": "x"})), as_user(&b))
            .await
            .is_err()
    );
    assert!(v.sleep_delete(p(json!({ "id": id })), as_user(&b)).await.is_err());

    // Delete cascades to the stages.
    v.sleep_delete(p(json!({ "id": id })), as_user(&a)).await.unwrap();
    let left = sql(
        &v,
        &a,
        "SELECT (SELECT count(*) FROM sleep_session) AS s, (SELECT count(*) FROM sleep_stage) AS g",
    )
    .await
    .unwrap();
    assert_eq!(left["rows"], json!([{"s": 0, "g": 0}]));
}

#[tokio::test]
async fn imported_sleep_is_read_only() {
    let _serial = SLEEP_WRITES.lock().await;
    let (v, db, owner) = setup().await;
    let a = uid("alice");
    // Importers bypass RLS; the owner (superuser in dev) stands in for one.
    let id: uuid::Uuid = sqlx::query_scalar(
        "WITH s AS (INSERT INTO vitals_data.sleep_session (user_id, source, start_at, end_at)
                    VALUES ($1, 'withings', '2026-10-07T23:00:00Z', '2026-10-08T07:00:00Z') RETURNING id)
         INSERT INTO vitals_data.sleep_stage (time, user_id, session_id, stage, duration_s)
         SELECT '2026-10-07T23:00:00Z', $1, id, 'deep', 1200 FROM s RETURNING session_id",
    )
    .bind(&a)
    .fetch_one(&owner)
    .await
    .unwrap();

    let stages = json!({"id": id, "stages": [{"time": "2026-10-08T01:00:00Z", "stage": "rem", "duration_s": 60}]});
    assert!(v.sleep_update(p(stages), as_user(&a)).await.is_err());
    assert!(
        v.sleep_update(p(json!({"id": id, "note": "x"})), as_user(&a))
            .await
            .is_err()
    );
    assert!(v.sleep_delete(p(json!({ "id": id })), as_user(&a)).await.is_err());

    // The tools filter on source themselves; this is what RLS stops if they didn't.
    let mut tx = db.app_tx(&a).await.unwrap();
    let raw = sqlx::query("UPDATE vitals_data.sleep_session SET note = 'x' WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(raw.rows_affected(), 0);
    let raw = sqlx::query("DELETE FROM vitals_data.sleep_stage WHERE session_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(raw.rows_affected(), 0);
    let ins = sqlx::query(
        "INSERT INTO vitals_data.sleep_stage (time, user_id, session_id, stage, duration_s)
         VALUES ('2026-10-08T01:00:00Z', $1, $2, 'rem', 60)",
    )
    .bind(&a)
    .bind(id)
    .execute(&mut *tx)
    .await;
    assert!(ins.unwrap_err().to_string().contains("row-level security"));
    drop(tx);

    assert_eq!(stage_seconds(&v, &a).await, json!({"deep": 1200}));
    let row = sql(&v, &a, "SELECT source, note FROM sleep_session").await.unwrap();
    assert_eq!(row["rows"], json!([{"source": "withings", "note": null}]));
}

// Ingest ----------------------------------------------------------------------

async fn post_ingest(db: &Db, user: &str, content_type: &str, encoding: Option<&str>, body: Vec<u8>) -> (u16, Value) {
    use tower::ServiceExt;
    let app: axum::Router = vitals_mcp::ingest::router(db.clone()).layer(axum::Extension(User(user.to_owned())));
    let mut req = Request::post("/v1/ingest").header("content-type", content_type);
    if let Some(e) = encoding {
        req = req.header("content-encoding", e);
    }
    let res = app
        .oneshot(req.body(axum::body::Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = res.status().as_u16();
    let body = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

fn polar_batch() -> Vec<u8> {
    [
        json!({"type": "hr", "device_id": "244E7234", "time": "2026-10-02T08:00:00.123456Z", "bpm": 61, "rr_ms": [980, 990]}),
        json!({"type": "ecg", "device_id": "244E7234", "time": "2026-10-02T08:00:00Z", "sample_rate": 130, "samples": [-12, 40, 7]}),
        json!({"type": "acc", "device_id": "244E7234", "time": "2026-10-02T08:00:00Z", "sample_rate": 200,
               "x": [1, 2], "y": [3, 4], "z": [-1000, -999]}),
        json!({"type": "battery", "device_id": "244E7234", "time": "2026-10-02T08:00:00Z", "level": 87}),
        json!({"type": "status", "device_id": "244E7234", "time": "2026-10-02T08:00:00Z", "event": "connected"}),
    ]
    .iter()
    .map(|f| f.to_string() + "\n")
    .collect::<String>()
    .into_bytes()
}

#[tokio::test]
async fn ingest_stores_once_and_shows_up_in_tools() {
    let (v, db, _) = setup().await;
    let a = uid("alice");
    assert_eq!(
        post_ingest(&db, &a, "application/x-ndjson", None, polar_batch()).await,
        (200, json!({"frames": 5, "inserted": 5}))
    );
    // A retried batch is a no-op, also zstd-compressed.
    let zstd = zstd::encode_all(&polar_batch()[..], 1).unwrap();
    assert_eq!(
        post_ingest(&db, &a, "application/x-ndjson", Some("zstd"), zstd).await,
        (200, json!({"frames": 5, "inserted": 0}))
    );

    let list: Value = serde_json::from_str(
        &v.reading_list(
            p(json!({"kind": "heart_rate", "source": "polar_h10", "from": "2026-10-01T00:00:00Z"})),
            as_user(&a),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(list[0]["bpm"], 61);
    assert_eq!(list[0]["rr_ms"], json!([980, 990]));
    let rows = sql(
        &v,
        &a,
        "SELECT (SELECT samples FROM ecg) AS ecg, (SELECT z FROM acc) AS z, (SELECT level FROM device_battery) AS level,
                (SELECT event FROM device_event) AS event, (SELECT rr_ms FROM heart_rate) AS rr_ms",
    )
    .await
    .unwrap();
    assert_eq!(
        rows["rows"][0],
        json!({"ecg": [-12, 40, 7], "z": [-1000, -999], "level": 87, "event": "connected", "rr_ms": [980, 990]})
    );
    // Another user sees none of it.
    let b = uid("bob");
    let rows = sql(
        &v,
        &b,
        "SELECT (SELECT count(*) FROM ecg) + (SELECT count(*) FROM heart_rate) AS n",
    )
    .await
    .unwrap();
    assert_eq!(rows["rows"][0]["n"], 0);
}

#[tokio::test]
async fn ingest_is_all_or_nothing() {
    let (v, db, _) = setup().await;
    let a = uid("alice");
    let mut body = polar_batch();
    body.extend_from_slice(br#"{"type":"hr","device_id":"d","time":"2026-10-02T08:00:01Z","bpm":0}"#);
    let (status, err) = post_ingest(&db, &a, "application/x-ndjson", None, body).await;
    assert_eq!((status, &err["frame"]), (400, &json!(5)));
    let rows = sql(
        &v,
        &a,
        "SELECT (SELECT count(*) FROM ecg) + (SELECT count(*) FROM heart_rate) AS n",
    )
    .await
    .unwrap();
    assert_eq!(rows["rows"][0]["n"], 0);
}

#[tokio::test]
async fn ingest_rejects_unsupported_requests() {
    let (_, db, _) = setup().await;
    let a = uid("alice");
    assert_eq!(
        post_ingest(&db, &a, "application/json", None, b"[]".to_vec()).await.0,
        415
    );
    assert_eq!(
        post_ingest(&db, &a, "application/x-ndjson", Some("br-nope"), polar_batch())
            .await
            .0,
        415
    );
    let huge = vec![b'\n'; vitals_mcp::ingest::MAX_BODY + 1];
    assert_eq!(post_ingest(&db, &a, "application/x-ndjson", None, huge).await.0, 413);
    // The cap applies after decompression: a small zstd body that inflates past it is refused too.
    let bomb = zstd::encode_all(&vec![b'\n'; vitals_mcp::ingest::MAX_BODY + 1][..], 19).unwrap();
    assert!(bomb.len() < 1 << 20);
    assert_eq!(
        post_ingest(&db, &a, "application/x-ndjson", Some("zstd"), bomb).await.0,
        413
    );
    // Empty heartbeat batches are fine.
    assert_eq!(post_ingest(&db, &a, "application/x-ndjson", None, vec![]).await.0, 200);
}

#[tokio::test]
async fn app_role_cannot_write_other_sources_or_users() {
    let (_, db, _) = setup().await;
    let (a, b) = (uid("alice"), uid("bob"));
    for (user, source) in [(&a, "evil"), (&b, "polar_h10")] {
        let mut tx = db.app_tx(&a).await.unwrap();
        let err =
            sqlx::query("INSERT INTO vitals_data.heart_rate (time, user_id, source, bpm) VALUES (now(), $1, $2, 60)")
                .bind(user)
                .bind(source)
                .execute(&mut *tx)
                .await
                .unwrap_err();
        assert!(err.to_string().contains("row-level security"), "{err}");
    }
    // Raw strap tables have no RLS (for compression); the app role may only insert, never read them.
    let mut tx = db.app_tx(&a).await.unwrap();
    let err = sqlx::query("SELECT 1 FROM vitals_data.ecg")
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("permission denied"), "{err}");
}

#[tokio::test]
async fn ingest_dedups_into_compressed_chunks() {
    let (_, db, owner) = setup().await;
    let a = uid("alice");
    // A day no other test writes to, so compressing its chunks can't disturb them.
    let frame = |kind: &str, sec: u32| {
        let t = format!("2026-09-15T08:00:{sec:02}Z");
        match kind {
            "ecg" => json!({"type": "ecg", "device_id": "d", "time": t, "sample_rate": 130, "samples": [1, 2]}),
            _ => json!({"type": "acc", "device_id": "d", "time": t, "sample_rate": 200, "x": [1], "y": [2], "z": [3]}),
        }
        .to_string()
            + "\n"
    };
    let batch = frame("ecg", 0) + &frame("acc", 0);
    let post = |body: String| post_ingest(&db, &a, "application/x-ndjson", None, body.into_bytes());
    assert_eq!(post(batch.clone()).await, (200, json!({"frames": 2, "inserted": 2})));

    for t in ["ecg", "acc"] {
        sqlx::query(
            "SELECT compress_chunk(c, if_not_compressed => true)
               FROM show_chunks($1::regclass, older_than => '2026-09-16'::timestamptz,
                                newer_than => '2026-09-15'::timestamptz) c",
        )
        .bind(format!("vitals_data.{t}"))
        .execute(&owner)
        .await
        .unwrap();
    }
    let compressed: bool = sqlx::query_scalar(
        "SELECT bool_and(is_compressed) FROM timescaledb_information.chunks
          WHERE hypertable_schema = 'vitals_data' AND hypertable_name IN ('ecg', 'acc')
            AND range_start = '2026-09-15'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert!(compressed);

    // The app role retries into the compressed chunks: duplicates are still skipped, new frames still land.
    assert_eq!(post(batch).await, (200, json!({"frames": 2, "inserted": 0})));
    assert_eq!(
        post(frame("ecg", 0) + &frame("ecg", 1)).await,
        (200, json!({"frames": 2, "inserted": 1}))
    );
}
