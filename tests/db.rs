//! Needs the dev database: `docker compose up -d`. Override URLs with TEST_OWNER_URL / TEST_APP_URL / TEST_READER_URL.

use axum::http::{Request, request::Parts};
use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool};
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
        reader: vitals_mcp::db::reader_pool_options()
            .connect(&url(
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
    assert_eq!(agg["truncated"], false);
    assert_eq!(agg["buckets"][0]["n"], 2);
    assert_eq!(agg["buckets"][0]["avg"], 65.0);

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
    let (v, _, owner) = setup().await;
    let a = uid("alice");
    // The owner (superuser in dev) stands in for an importer.
    sqlx::query("INSERT INTO vitals_data.heart_rate VALUES ('2026-10-02T07:00:00Z', $1, 'import', 55, null)")
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

    let rows = sql(&v, &a, "SELECT bpm, source FROM heart_rate").await.unwrap();
    assert_eq!(rows["rows"], json!([{"bpm": 55, "source": "import"}]));
}

#[tokio::test]
async fn other_users_rows_are_invisible_and_untouchable() {
    let (v, _, _) = setup().await;
    let (a, b) = (uid("alice"), uid("bob"));
    add_hr(&v, &a, "2026-10-03T07:00:00Z", 61).await;
    add_hr(&v, &b, "2026-10-03T08:00:00Z", 99).await;

    // No RLS: every tool filters on the caller itself. Bob targets alice's reading.
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

    // The read tools show bob only his own reading.
    let list: Value = serde_json::from_str(
        &v.reading_list(
            p(json!({"kind": "heart_rate", "from": "2026-10-01T00:00:00Z", "to": "2026-10-05T00:00:00Z"})),
            as_user(&b),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1, "{list}");
    assert_eq!(list[0]["bpm"], 99);
    let agg: Value = serde_json::from_str(
        &v.reading_aggregate(
            p(json!({"kind": "heart_rate", "field": "bpm",
                     "from": "2026-10-01T00:00:00Z", "to": "2026-10-05T00:00:00Z"})),
            as_user(&b),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        (&agg["buckets"][0]["n"], &agg["buckets"][0]["max"]),
        (&json!(1), &json!(99)),
        "{agg}"
    );
    // Alice has a reading and a night inside the summary's 30-day window; bob's summary shows none of it.
    let _serial = SLEEP_WRITES.lock().await;
    let ago = |h: i64| (chrono::Utc::now() - chrono::Duration::hours(h)).to_rfc3339();
    add_hr(&v, &a, &ago(1), 61).await;
    v.sleep_add(
        p(json!({"start_at": ago(10), "end_at": ago(2),
                 "stages": [{"time": ago(10), "stage": "deep", "duration_s": 600}]})),
        as_user(&a),
    )
    .await
    .unwrap();
    let summary: Value = serde_json::from_str(&v.summary(as_user(&b)).await.unwrap()).unwrap();
    assert_eq!(summary["heart_rate"]["latest"]["bpm"], 99, "{summary}");
    assert_eq!(summary["heart_rate"]["bpm"]["n_30d"], 0, "{summary}");
    assert_eq!(summary["sleep"]["nights_30d"], 0, "{summary}");
    assert_eq!(summary["sleep"]["stage_pct_30d"], Value::Null, "{summary}");

    // Alice's sleep: bob can't see, change or delete it.
    let id = session_id(
        &v.sleep_add(
            p(
                json!({"start_at": "2026-10-03T23:00:00Z", "end_at": "2026-10-04T07:00:00Z",
                     "stages": [{"time": "2026-10-03T23:00:00Z", "stage": "deep", "duration_s": 600}]}),
            ),
            as_user(&a),
        )
        .await
        .unwrap(),
    );
    let bobs_nights = v
        .sleep_list(
            p(json!({"from": "2026-10-01T00:00:00Z", "to": "2026-10-05T00:00:00Z"})),
            as_user(&b),
        )
        .await
        .unwrap();
    assert_eq!(bobs_nights, "[]");
    let stages = json!({"id": id, "stages": [{"time": "2026-10-04T01:00:00Z", "stage": "rem", "duration_s": 60}]});
    assert!(v.sleep_update(p(stages), as_user(&b)).await.is_err());
    assert!(v.sleep_delete(p(json!({ "id": id })), as_user(&b)).await.is_err());
    let nights: Value = serde_json::from_str(
        &v.sleep_list(
            p(json!({"from": "2026-10-01T00:00:00Z", "to": "2026-10-05T00:00:00Z"})),
            as_user(&a),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(nights[0]["stage_seconds"], json!({"deep": 600}), "{nights}");
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
async fn sql_query_does_not_leak_session_state() {
    // A one-connection reader pool forces the same connection to be reused, so a session advisory lock
    // that survived `sql_query`'s rollback would still be held on the next call. The release hook scrubs it.
    let (_, db0, _) = setup().await;
    let reader = vitals_mcp::db::reader_pool_options()
        .max_connections(1)
        .connect(&url(
            "TEST_READER_URL",
            "postgres://vitals_reader:reader@localhost:55432/vitals",
        ))
        .await
        .unwrap();
    let v = Vitals::new(Db {
        app: db0.app.clone(),
        reader,
    });
    let u = uid("lock");
    sql(&v, &u, "SELECT pg_advisory_lock(42) AS x").await.unwrap();
    // Same pooled connection; the lock must have been released on return, so this session no longer holds it.
    let r = sql(&v, &u, "SELECT pg_advisory_unlock(42) AS released").await.unwrap();
    assert_eq!(r["rows"][0]["released"], json!(false));
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
    // A night just outside the 30-day window: in neither the night count nor the stage split.
    let old = |m: i64| (start - chrono::Duration::days(30) - chrono::Duration::minutes(m)).to_rfc3339();
    v.sleep_add(
        p(json!({"start_at": old(480), "end_at": old(0),
                 "stages": [{"time": old(480), "stage": "awake", "duration_s": 600}]})),
        as_user(&a),
    )
    .await
    .unwrap();

    let nights: Value = serde_json::from_str(&v.sleep_list(p(json!({})), as_user(&a)).await.unwrap()).unwrap();
    assert_eq!(nights[0]["hours"], 8.0);
    assert_eq!(nights[0]["stage_seconds"]["deep"], 1800);

    let s: Value = serde_json::from_str(&v.summary(as_user(&a)).await.unwrap()).unwrap();
    assert_eq!(s["heart_rate"]["latest"]["bpm"], 58);
    assert_eq!(s["sleep"]["nights_30d"], 1);
    assert_eq!(
        s["sleep"]["stage_pct_30d"],
        json!({"light": 50.0, "deep": 25.0, "rem": 25.0}),
        "{s}"
    );
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

    // rmcp parses tool arguments with this same serde_json::from_value (-> invalid_params), so sleep_update never runs.
    let typo = serde_json::from_value::<SleepUpdate>(json!({"id": id, "notes": "x"}));
    assert!(typo.unwrap_err().to_string().contains("unknown field `notes`"));

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
    let (v, _, owner) = setup().await;
    let a = uid("alice");
    // The owner (superuser in dev) stands in for an importer.
    let id: uuid::Uuid = sqlx::query_scalar(
        "WITH s AS (INSERT INTO vitals_data.sleep_session (user_id, source, start_at, end_at)
                    VALUES ($1, 'import', '2026-10-07T23:00:00Z', '2026-10-08T07:00:00Z') RETURNING id)
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

    assert_eq!(stage_seconds(&v, &a).await, json!({"deep": 1200}));
    let row = sql(&v, &a, "SELECT source, note FROM sleep_session").await.unwrap();
    assert_eq!(row["rows"], json!([{"source": "import", "note": null}]));
}

// Ingest ----------------------------------------------------------------------

async fn api(db: &Db, user: &str, req: Request<axum::body::Body>) -> (u16, Value) {
    use tower::ServiceExt;
    let app: axum::Router = vitals_mcp::ingest::router(db.clone()).layer(axum::Extension(User(user.to_owned())));
    let res = app.oneshot(req).await.unwrap();
    let status = res.status().as_u16();
    let body = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

async fn post_ingest(db: &Db, user: &str, content_type: &str, encoding: Option<&str>, body: Vec<u8>) -> (u16, Value) {
    let mut req = Request::post("/v1/ingest").header("content-type", content_type);
    if let Some(e) = encoding {
        req = req.header("content-encoding", e);
    }
    api(db, user, req.body(axum::body::Body::from(body)).unwrap()).await
}

async fn list_devices(db: &Db, user: &str) -> (u16, Value) {
    api(
        db,
        user,
        Request::get("/v1/devices").body(axum::body::Body::empty()).unwrap(),
    )
    .await
}

async fn register(db: &Db, user: &str, model: &str, serial: &str) -> i64 {
    let req = Request::post("/v1/devices")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            json!({"model": model, "serial": serial}).to_string(),
        ))
        .unwrap();
    let (status, body) = api(db, user, req).await;
    assert_eq!(status, 200, "{body}");
    body["id"].as_i64().unwrap()
}

fn polar_batch(device: i64) -> Vec<u8> {
    [
        json!({"type": "hr", "device_id": device, "time": "2026-10-02T08:00:00.123456Z", "bpm": 61, "rr_ms": [980, 990]}),
        json!({"type": "ecg", "device_id": device, "time": "2026-10-02T08:00:00Z", "sample_rate": 130, "samples": [-12, 40, 7]}),
        json!({"type": "acc", "device_id": device, "time": "2026-10-02T08:00:00Z", "sample_rate": 200,
               "x": [1, 2], "y": [3, 4], "z": [-1000, -999]}),
        json!({"type": "battery", "device_id": device, "time": "2026-10-02T08:00:00Z", "level": 87}),
        json!({"type": "status", "device_id": device, "time": "2026-10-02T08:00:00Z", "event": "connected"}),
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
    let h10 = register(&db, &a, "Polar H10", "244E7234").await;
    assert_eq!(
        post_ingest(&db, &a, "application/x-ndjson", None, polar_batch(h10)).await,
        (200, json!({"frames": 5, "inserted": 5}))
    );
    // A retried batch is a no-op, also zstd-compressed.
    let zstd = zstd::encode_all(&polar_batch(h10)[..], 1).unwrap();
    assert_eq!(
        post_ingest(&db, &a, "application/x-ndjson", Some("zstd"), zstd).await,
        (200, json!({"frames": 5, "inserted": 0}))
    );

    let list: Value = serde_json::from_str(
        &v.reading_list(
            p(json!({"kind": "heart_rate", "source": "app", "from": "2026-10-01T00:00:00Z"})),
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
        "SELECT (SELECT array_agg(uv ORDER BY time) FROM ecg) AS ecg, (SELECT array_agg(z ORDER BY time) FROM acc) AS z,
                (SELECT array_agg(to_char(time AT TIME ZONE 'UTC', 'SS.US') ORDER BY time) FROM ecg) AS ecg_t,
                (SELECT array_agg(to_char(time AT TIME ZONE 'UTC', 'SS.US') ORDER BY time) FROM acc) AS acc_t,
                (SELECT level FROM device_battery) AS level, (SELECT event FROM device_event) AS event,
                (SELECT rr_ms FROM heart_rate) AS rr_ms",
    )
    .await
    .unwrap();
    assert_eq!(
        rows["rows"][0],
        json!({"ecg": [-12, 40, 7], "z": [-1000, -999],
               // One row per sample: sample i at time + i / sample_rate, rounded to µs.
               "ecg_t": ["00.000000", "00.007692", "00.015385"], "acc_t": ["00.000000", "00.005000"],
               "level": 87, "event": "connected", "rr_ms": [980, 990]})
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
    let h10 = register(&db, &a, "Polar H10", "244E7234").await;
    let mut body = polar_batch(h10);
    body.extend_from_slice(
        format!(r#"{{"type":"hr","device_id":{h10},"time":"2026-10-02T08:00:01Z","bpm":0}}"#).as_bytes(),
    );
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
async fn devices_are_per_user_and_registration_is_idempotent() {
    let (v, db, _) = setup().await;
    let (a, b) = (uid("alice"), uid("bob"));
    let h10 = register(&db, &a, "Polar H10", "244E7234").await;
    assert_eq!(register(&db, &a, "Polar H10", "244E7234").await, h10);
    let loop_ = register(&db, &a, "Polar Loop", "244E7234").await;
    assert_ne!(loop_, h10);
    assert_eq!(
        list_devices(&db, &a).await,
        (
            200,
            json!([{"id": h10, "model": "Polar H10", "serial": "244E7234"},
                   {"id": loop_, "model": "Polar Loop", "serial": "244E7234"}])
        )
    );
    assert_eq!(list_devices(&db, &b).await, (200, json!([])));
    assert_eq!(
        sql(&v, &b, "SELECT count(*) AS n FROM devices").await.unwrap()["rows"][0]["n"],
        0
    );

    // Bob can't write into Alice's device, nor into one that doesn't exist; nothing of the batch is stored.
    for device in [h10, i64::from(i32::MAX)] {
        let (status, err) = post_ingest(&db, &b, "application/x-ndjson", None, polar_batch(device)).await;
        assert_eq!((status, &err["frame"]), (400, &json!(0)), "{err}");
    }
    let rows = sql(
        &v,
        &a,
        "SELECT (SELECT count(*) FROM ecg) + (SELECT count(*) FROM heart_rate) AS n",
    )
    .await
    .unwrap();
    assert_eq!(rows["rows"][0]["n"], 0);

    let bad = Request::post("/v1/devices")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(r#"{"model":"","serial":"x"}"#))
        .unwrap();
    assert_eq!(api(&db, &a, bad).await.0, 400);
}

#[tokio::test]
async fn two_trackers_at_the_same_instant_are_two_heart_rate_rows() {
    let (v, db, _) = setup().await;
    let a = uid("alice");
    let body: String = [
        register(&db, &a, "Polar H10", "A").await,
        register(&db, &a, "Polar Loop", "B").await,
    ]
    .iter()
    .map(|d| json!({"type": "hr", "device_id": d, "time": "2026-10-03T08:00:00Z", "bpm": 60}).to_string() + "\n")
    .collect();
    assert_eq!(
        post_ingest(&db, &a, "application/x-ndjson", None, body.into_bytes()).await,
        (200, json!({"frames": 2, "inserted": 2}))
    );
    let rows = sql(
        &v,
        &a,
        "SELECT d.model FROM heart_rate h JOIN devices d ON d.id = h.device_id ORDER BY 1",
    )
    .await
    .unwrap();
    assert_eq!(rows["rows"], json!([{"model": "Polar H10"}, {"model": "Polar Loop"}]));
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
        post_ingest(&db, &a, "application/x-ndjson", Some("br-nope"), polar_batch(1))
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
async fn app_role_cannot_read_raw_tables() {
    let (_, db, _) = setup().await;
    // The app role may only insert into the raw strap tables, never read them.
    let mut tx = db.app_tx().await.unwrap();
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
    let d = register(&db, &a, "Polar H10", "244E7234").await;
    // An hour no other test writes to, so compressing its chunks can't disturb them.
    let frame = |kind: &str, sec: u32| {
        let t = format!("2026-09-15T08:00:{sec:02}Z");
        match kind {
            "ecg" => json!({"type": "ecg", "device_id": d, "time": t, "sample_rate": 130, "samples": [1, 2]}),
            _ => json!({"type": "acc", "device_id": d, "time": t, "sample_rate": 200, "x": [1], "y": [2], "z": [3]}),
        }
        .to_string()
            + "\n"
    };
    let batch = frame("ecg", 0) + &frame("acc", 0);
    let post = |body: String| post_ingest(&db, &a, "application/x-ndjson", None, body.into_bytes());
    assert_eq!(post(batch.clone()).await, (200, json!({"frames": 2, "inserted": 2})));

    // The compression policy may compress the same chunk concurrently (it runs right after migrating a
    // fresh DB); compress_chunk then raises 42710 despite if_not_compressed. The assertion below is the real check.
    sqlx::query(
        "DO $$ DECLARE c regclass; BEGIN
           FOR c IN SELECT show_chunks(t, older_than => '2026-09-15 09:00Z'::timestamptz,
                                        newer_than => '2026-09-15 08:00Z'::timestamptz)
                      FROM unnest('{vitals_data.ecg,vitals_data.acc}'::regclass[]) t LOOP
             BEGIN PERFORM compress_chunk(c, if_not_compressed => true);
             EXCEPTION WHEN duplicate_object THEN NULL;
             END;
           END LOOP;
         END $$",
    )
    .execute(&owner)
    .await
    .unwrap();
    let compressed: bool = sqlx::query_scalar(
        "SELECT bool_and(is_compressed) FROM timescaledb_information.chunks
          WHERE hypertable_schema = 'vitals_data' AND hypertable_name IN ('ecg', 'acc')
            AND range_start = '2026-09-15 08:00Z'",
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

/// Compresses the chunks of `tables` that contain `day` (owner role; tolerates the compression policy racing it).
async fn compress_day(owner: &PgPool, tables: &[&str], day: &str) {
    for t in tables {
        sqlx::query(AssertSqlSafe(format!(
            "DO $$ DECLARE c regclass; BEGIN
               FOR c IN SELECT show_chunks('vitals_data.{t}', older_than => '{day}'::timestamptz + interval '8 days',
                                           newer_than => '{day}'::timestamptz - interval '7 days') LOOP
                 BEGIN PERFORM compress_chunk(c, if_not_compressed => true);
                 EXCEPTION WHEN duplicate_object THEN NULL;
                 END;
               END LOOP;
             END $$"
        )))
        .execute(owner)
        .await
        .unwrap();
        let compressed: bool = sqlx::query_scalar(
            "SELECT bool_and(is_compressed) FROM timescaledb_information.chunks
              WHERE hypertable_schema = 'vitals_data' AND hypertable_name = $1
                AND range_start <= $2::timestamptz AND range_end > $2::timestamptz",
        )
        .bind(t)
        .bind(day)
        .fetch_one(owner)
        .await
        .unwrap();
        assert!(compressed, "{t}");
    }
}

#[tokio::test]
async fn edits_and_resends_work_on_compressed_chunks() {
    let _serial = SLEEP_WRITES.lock().await;
    let (v, db, owner) = setup().await;
    let a = uid("alice");
    let d = register(&db, &a, "Polar Loop", "C").await;
    // A day no other test writes to, so compressing its chunks can't disturb them.
    let t = "2026-08-20T08:00:00Z";
    let batch = [
        json!({"type": "hr", "device_id": d, "time": t, "bpm": 60}),
        json!({"type": "battery", "device_id": d, "time": t, "level": 80}),
        json!({"type": "status", "device_id": d, "time": t, "event": "connected"}),
        json!({"type": "skin_temp", "device_id": d, "time": t, "sample_rate": 1, "celsius": [33.5]}),
        json!({"type": "sleep", "device_id": d, "start_at": "2026-08-19T23:00:00Z", "end_at": "2026-08-20T07:00:00Z",
               "stages": [{"time": "2026-08-19T23:00:00Z", "stage": "light", "duration_s": 600}]}),
    ]
    .iter()
    .map(|f| f.to_string() + "\n")
    .collect::<String>();
    let post = |body: String| post_ingest(&db, &a, "application/x-ndjson", None, body.into_bytes());
    assert_eq!(post(batch.clone()).await, (200, json!({"frames": 5, "inserted": 5})));
    for (kind, values) in [("heart_rate", json!({"bpm": 61})), ("hrv", json!({"rmssd_ms": 40}))] {
        let mut r = values;
        r["time"] = json!("2026-08-20T09:00:00Z");
        v.reading_add(p(json!({"kind": kind, "readings": [r]})), as_user(&a))
            .await
            .unwrap();
    }
    let id = session_id(
        &v.sleep_add(
            p(
                json!({"start_at": "2026-08-20T13:00:00Z", "end_at": "2026-08-20T14:00:00Z",
                     "stages": [{"time": "2026-08-20T13:00:00Z", "stage": "light", "duration_s": 600}]}),
            ),
            as_user(&a),
        )
        .await
        .unwrap(),
    );

    compress_day(
        &owner,
        &[
            "heart_rate",
            "hrv",
            "device_battery",
            "device_event",
            "skin_temp",
            "sleep_stage",
        ],
        "2026-08-20",
    )
    .await;

    // Resends still deduplicate, new frames still land.
    assert_eq!(post(batch).await, (200, json!({"frames": 5, "inserted": 0})));
    let new = json!({"type": "hr", "device_id": d, "time": "2026-08-20T08:00:01Z", "bpm": 62}).to_string();
    assert_eq!(post(new).await, (200, json!({"frames": 1, "inserted": 1})));
    // A manual row (NULL device_id, a segmentby column) is still unique once compressed.
    let dup = v
        .reading_add(
            p(json!({"kind": "heart_rate", "readings": [{"time": "2026-08-20T09:00:00Z", "bpm": 61}]})),
            as_user(&a),
        )
        .await;
    assert!(dup.unwrap_err().contains("duplicate key"));
    // Manual edits still work.
    v.reading_update(
        p(json!({"kind": "heart_rate", "time": "2026-08-20T09:00:00Z", "set": {"bpm": 70}})),
        as_user(&a),
    )
    .await
    .unwrap();
    let del = v
        .reading_delete(
            p(json!({"kind": "hrv", "times": ["2026-08-20T09:00:00Z"]})),
            as_user(&a),
        )
        .await
        .unwrap();
    assert_eq!(del, "deleted 1 manual hrv reading(s)");
    let stages = json!({"id": id, "stages": [{"time": "2026-08-20T13:00:00Z", "stage": "rem", "duration_s": 300}]});
    v.sleep_update(p(stages), as_user(&a)).await.unwrap();

    let rows = sql(
        &v,
        &a,
        "SELECT (SELECT array_agg(bpm ORDER BY time) FROM heart_rate) AS bpm,
                (SELECT count(*) FROM hrv) AS hrv,
                (SELECT array_agg(stage || duration_s ORDER BY time) FROM sleep_stage) AS stages",
    )
    .await
    .unwrap();
    assert_eq!(
        rows["rows"][0],
        json!({"bpm": [60, 62, 70], "hrv": 0, "stages": ["light600", "rem300"]})
    );

    // sleep_delete cascades into the compressed stages.
    compress_day(&owner, &["sleep_stage"], "2026-08-20").await;
    v.sleep_delete(p(json!({ "id": id })), as_user(&a)).await.unwrap();
    let left = sql(
        &v,
        &a,
        &format!("SELECT count(*) AS n FROM sleep_stage WHERE session_id = '{id}'"),
    )
    .await
    .unwrap();
    assert_eq!(left["rows"][0]["n"], 0);
}

#[tokio::test]
async fn reading_aggregate_caps_buckets_and_app_queries_time_out() {
    let (v, db, _) = setup().await;
    let a = uid("alice");
    let readings: Vec<Value> = (0..1001)
        .map(|i| json!({"time": format!("2026-09-20T10:{:02}:{:02}Z", i / 60, i % 60), "bpm": 60}))
        .collect();
    // reading_add takes at most 1000 per call.
    for chunk in readings.chunks(1000) {
        v.reading_add(p(json!({"kind": "heart_rate", "readings": chunk})), as_user(&a))
            .await
            .unwrap();
    }
    let agg: Value = serde_json::from_str(
        &v.reading_aggregate(
            p(json!({"kind": "heart_rate", "field": "bpm", "bucket": "1 second",
                     "from": "2026-09-20T00:00:00Z", "to": "2026-09-21T00:00:00Z"})),
            as_user(&a),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(agg["truncated"], true);
    assert_eq!(agg["buckets"].as_array().unwrap().len(), 1000);

    let mut tx = db.app_tx().await.unwrap();
    let timeout: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(timeout, "30s");
}

#[tokio::test]
async fn sql_query_accepts_a_column_named_like_its_wrapper_alias() {
    let (v, _, _) = setup().await;
    let rows = sql(&v, &uid("alice"), "SELECT 1 AS t, 2 AS n").await.unwrap();
    assert_eq!(rows["rows"], json!([{ "t": 1, "n": 2 }]));
}

#[tokio::test]
async fn loop_frames_round_trip() {
    let _serial = SLEEP_WRITES.lock().await;
    let (v, db, _) = setup().await;
    let a = uid("alice");
    let lp = register(&db, &a, "Polar Loop", "C0FFEE01").await;
    let body = [
        json!({"type": "skin_temp", "device_id": lp, "time": "2026-10-03T01:00:00Z", "sample_rate": 1,
               "celsius": [33.25, 33.5]}),
        json!({"type": "sleep", "device_id": lp, "start_at": "2026-10-02T23:00:00Z", "end_at": "2026-10-03T07:00:00Z",
               "stages": [{"time": "2026-10-02T23:00:00Z", "stage": "light", "duration_s": 3600},
                          {"time": "2026-10-03T00:00:00Z", "stage": "deep", "duration_s": 1800}]}),
        json!({"type": "recharge", "device_id": lp, "night": "2026-10-03", "status": 4, "ans_charge": 2.5,
               "hrv_ms": 48, "breathing_rate": 14.5, "hr_bpm": 52}),
    ]
    .iter()
    .map(|f| f.to_string() + "\n")
    .collect::<String>()
    .into_bytes();
    assert_eq!(
        post_ingest(&db, &a, "application/x-ndjson", None, body.clone()).await,
        (200, json!({"frames": 3, "inserted": 3}))
    );
    // A re-sent night adds neither a session nor stages.
    assert_eq!(
        post_ingest(&db, &a, "application/x-ndjson", None, body).await,
        (200, json!({"frames": 3, "inserted": 0}))
    );

    let rows = sql(
        &v,
        &a,
        "SELECT (SELECT celsius FROM skin_temp) AS celsius, (SELECT ans_charge FROM recharge) AS ans,
                (SELECT device_id FROM sleep_session) AS device, (SELECT count(*) FROM sleep_stage) AS stages",
    )
    .await
    .unwrap();
    assert_eq!(
        rows["rows"][0],
        json!({"celsius": [33.25, 33.5], "ans": 2.5, "device": lp, "stages": 2})
    );
    assert_eq!(stage_seconds(&v, &a).await, json!({"light": 3600, "deep": 1800}));

    // App sleep is read-only for the MCP write tools.
    let id = sql(&v, &a, "SELECT id FROM sleep_session").await.unwrap()["rows"][0]["id"].clone();
    let err = v.sleep_delete(p(json!({ "id": id })), as_user(&a)).await.unwrap_err();
    assert!(err.contains("no manual sleep session"), "{err}");
    let err = v
        .sleep_update(p(json!({"id": id, "note": "x"})), as_user(&a))
        .await
        .unwrap_err();
    assert!(err.contains("no manual sleep session"), "{err}");
}

// App access to the tools ------------------------------------------------------

async fn tool(db: &Db, user: &str, name: &str, args: Value) -> (u16, Value) {
    let req = Request::post(format!("/v1/tools/{name}"))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(args.to_string()))
        .unwrap();
    api(db, user, req).await
}

#[tokio::test]
async fn app_can_call_every_tool() {
    let (_, db, _) = setup().await;
    let (a, b) = (uid("alice"), uid("bob"));
    // Whole seconds: Postgres keeps microseconds, and update/delete match the exact time.
    let t = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let hr = |extra: Value| {
        let mut m = json!({"kind": "heart_rate"});
        m.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        m
    };

    let (s, out) = tool(
        &db,
        &a,
        "reading_add",
        hr(json!({"readings": [{"time": t, "bpm": 64}]})),
    )
    .await;
    assert_eq!((s, out), (200, json!("added 1 heart_rate reading(s)")));
    let (s, out) = tool(&db, &a, "reading_list", hr(json!({}))).await;
    assert_eq!((s, out[0]["bpm"].clone()), (200, json!(64)));
    let (s, out) = tool(&db, &a, "reading_aggregate", hr(json!({"field": "bpm"}))).await;
    assert_eq!((s, out["buckets"][0]["n"].clone()), (200, json!(1)));
    let (s, out) = tool(&db, &a, "summary", Value::Null).await;
    assert_eq!((s, out["heart_rate"]["latest"]["bpm"].clone()), (200, json!(64)));
    // A truly empty body means `{}`.
    let empty = Request::post("/v1/tools/summary")
        .body(axum::body::Body::empty())
        .unwrap();
    let (s, out) = api(&db, &a, empty).await;
    assert_eq!((s, out["heart_rate"]["latest"]["bpm"].clone()), (200, json!(64)));
    let (s, out) = tool(&db, &a, "schema_describe", json!({})).await;
    assert!(s == 200 && out.as_str().unwrap().contains("heart_rate"), "{s} {out}");
    let (s, out) = tool(&db, &a, "sql_query", json!({"sql": "SELECT bpm FROM heart_rate"})).await;
    assert_eq!((s, out["rows"].clone()), (200, json!([{"bpm": 64}])));

    // Bob reaches none of it.
    assert_eq!(tool(&db, &b, "reading_list", hr(json!({}))).await, (200, json!([])));
    let (_, out) = tool(&db, &b, "sql_query", json!({"sql": "SELECT bpm FROM heart_rate"})).await;
    assert_eq!(out["rows"], json!([]));

    let (s, out) = tool(&db, &a, "reading_update", hr(json!({"time": t, "set": {"bpm": 66}}))).await;
    assert_eq!(s, 200, "{out}");
    let (s, out) = tool(&db, &a, "reading_delete", hr(json!({"times": [t]}))).await;
    assert_eq!((s, out), (200, json!("deleted 1 manual heart_rate reading(s)")));

    {
        let _serial = SLEEP_WRITES.lock().await;
        let range = json!({"from": "2026-06-14T00:00:00Z", "to": "2026-06-16T00:00:00Z"});
        let (s, out) = tool(
            &db,
            &a,
            "sleep_add",
            json!({"start_at": "2026-06-14T23:00:00Z", "end_at": "2026-06-15T07:00:00Z",
                   "stages": [{"time": "2026-06-14T23:00:00Z", "stage": "deep", "duration_s": 600}]}),
        )
        .await;
        assert_eq!(s, 200, "{out}");
        let id = session_id(out.as_str().unwrap());
        let (s, out) = tool(&db, &a, "sleep_list", range.clone()).await;
        assert_eq!((s, out[0]["stage_seconds"].clone()), (200, json!({"deep": 600})));
        let (s, out) = tool(&db, &a, "sleep_update", json!({"id": id, "note": "late"})).await;
        assert_eq!(s, 200, "{out}");
        let (s, out) = tool(&db, &a, "sleep_delete", json!({ "id": id })).await;
        assert_eq!(s, 200, "{out}");
        assert_eq!(tool(&db, &a, "sleep_list", range).await, (200, json!([])));
    }

    assert_eq!(tool(&db, &a, "nope", json!({})).await.0, 404);
    let (s, out) = tool(&db, &a, "reading_list", hr(json!({"bogus": 1}))).await;
    assert!(
        s == 400 && out["error"].as_str().unwrap().contains("bogus"),
        "{s} {out}"
    );
    let (s, out) = tool(&db, &a, "reading_aggregate", hr(json!({"field": "kg"}))).await;
    assert!(
        s == 400 && out["error"].as_str().unwrap().contains("does not belong"),
        "{s} {out}"
    );

    // Tool bodies get 1 MiB, not ingest's 32 MiB, also after decompression.
    let pad = "x".repeat(vitals_mcp::ingest::MAX_TOOL_BODY);
    assert_eq!(tool(&db, &a, "summary", json!({ "pad": pad })).await.0, 413);
    let bomb = zstd::encode_all(json!({ "pad": pad }).to_string().as_bytes(), 19).unwrap();
    let req = Request::post("/v1/tools/summary")
        .header("content-encoding", "zstd")
        .body(axum::body::Body::from(bomb))
        .unwrap();
    assert_eq!(api(&db, &a, req).await.0, 413);
    // reading_add takes at most 1000 readings per call (well under the body limit).
    let many: Vec<_> = (0..1001)
        .map(|i| json!({"time": format!("2026-01-01T00:00:{:02}Z", i % 60), "bpm": 60}))
        .collect();
    let (s, out) = tool(&db, &a, "reading_add", hr(json!({ "readings": many }))).await;
    assert!(
        s == 400 && out["error"].as_str().unwrap().contains("at most 1000 readings"),
        "{s} {out}"
    );
}

#[tokio::test]
async fn app_gets_the_tool_schemas() {
    let (_, db, _) = setup().await;
    let (s, out) = api(
        &db,
        "x",
        Request::get("/v1/tools").body(axum::body::Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, 200);
    let mut names: Vec<_> = out
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "reading_add",
            "reading_aggregate",
            "reading_delete",
            "reading_list",
            "reading_update",
            "schema_describe",
            "sleep_add",
            "sleep_delete",
            "sleep_list",
            "sleep_update",
            "sql_query",
            "summary"
        ]
    );
    assert!(out.as_array().unwrap().iter().all(|t| t["inputSchema"].is_object()));
    // Every listed tool is also callable: `Vitals::call` is a hand-written match next to `#[tool_router]`.
    // A bad argument gets 400 (or 200 from tools without arguments); a missing arm in `call` gets 404.
    for name in names {
        let (s, out) = tool(&db, "x", name, json!({ "__probe": 1 })).await;
        assert!(matches!(s, 200 | 400), "{name}: expected 200 or 400, got {s}: {out}");
    }
}
