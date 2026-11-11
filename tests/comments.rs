//! Comment threads on workouts, exercises and sets, through `/api` with signed test tokens, including people the
//! owner shares gym data with. Needs the dev database: `docker compose up -d`.

mod common;

use axum::body::Body;
use axum::http::{Request, header};
use common::*;
use serde_json::{Value, json};
use tower::ServiceExt;

/// `g` accepts a share of `o`'s data at these levels.
async fn share(db: &health_mcp::db::Db, o: &Person, g: &Person, gym: &str, vitals: &str) {
    let (id, _) = invite(db, o, g, gym, vitals).await;
    ok(db, g, "share_respond", json!({"invite_id": id, "accept": true})).await;
}

/// The one workout and one set of `owner_with_data`.
async fn ids(db: &health_mcp::db::Db, o: &Person) -> (i64, i64) {
    let rows = ok(
        db,
        o,
        "sql_query",
        json!({"sql": "SELECT w.id AS w, s.id AS s FROM workout w JOIN workout_set s ON s.workout_id = w.id"}),
    )
    .await;
    let r = &rows["rows"][0];
    (r["w"].as_i64().unwrap(), r["s"].as_i64().unwrap())
}

fn with_owner(o: &Person, mut args: Value) -> Value {
    args["owner"] = json!(o.email());
    args
}

#[tokio::test]
async fn threads_are_bound_to_their_target() {
    let db = db(None).await;
    let o = owner_with_data(&db).await;
    let (workout, set) = ids(&db, &o).await;

    let w = ok(
        &db,
        &o,
        "thread_start",
        json!({"target": "workout", "id": workout, "body": "Felt strong"}),
    )
    .await;
    assert_eq!(
        (w["target"].clone(), w["workout_id"].clone()),
        (json!("workout"), json!(workout))
    );
    let e = ok(
        &db,
        &o,
        "thread_start",
        json!({"target": "exercise", "exercise": "squat", "body": "Form check?"}),
    )
    .await;
    assert_eq!(e["exercise"], "Squat");
    let s = ok(
        &db,
        &o,
        "thread_start",
        json!({"target": "set", "id": set, "body": "Too heavy"}),
    )
    .await;
    assert_eq!(
        (s["set_id"].clone(), s["exercise"].clone()),
        (json!(set), json!("Squat"))
    );
    assert_eq!(s["comments"][0]["body"], "Too heavy");
    assert_eq!(s["comments"][0]["by_owner"], true);

    // Per target, only that target's threads.
    let on_set = ok(&db, &o, "comment_list", json!({"target": "set", "id": set})).await;
    assert_eq!(on_set.as_array().unwrap().len(), 1);
    assert_eq!(on_set[0]["thread_id"], s["thread_id"]);
    let all = ok(&db, &o, "comment_list", json!({})).await;
    assert_eq!(all.as_array().unwrap().len(), 3);
    let e2 = err(&db, &o, "comment_list", json!({"id": set})).await;
    assert!(e2.contains("give the target"), "{e2}");

    // Ids from someone else's logbook are not found; empty comments are refused.
    let other = owner_with_data(&db).await;
    let e2 = err(
        &db,
        &other,
        "thread_start",
        json!({"target": "set", "id": set, "body": "hi"}),
    )
    .await;
    assert!(e2.contains("no set with id"), "{e2}");
    let e2 = err(
        &db,
        &o,
        "thread_start",
        json!({"target": "set", "id": set, "body": "  "}),
    )
    .await;
    assert!(e2.contains("empty"), "{e2}");

    // Deleting the set removes its threads.
    ok(&db, &o, "set_delete", json!({"set_ids": [set]})).await;
    let on_set = ok(&db, &o, "comment_list", json!({"target": "set"})).await;
    assert_eq!(on_set, json!([]));
}

#[tokio::test]
async fn shared_users_comment_with_owner() {
    let db = db(None).await;
    let o = owner_with_data(&db).await;
    let (_, set) = ids(&db, &o).await;
    let (trainer, doctor, stranger) = (person("trainer"), person("doctor"), person("stranger"));
    share(&db, &o, &trainer, "ro", "none").await;
    share(&db, &o, &doctor, "none", "ro").await;

    let t = ok(
        &db,
        &o,
        "thread_start",
        json!({"target": "set", "id": set, "body": "Was this deep enough?"}),
    )
    .await;
    let thread = t["thread_id"].as_i64().unwrap();
    // Read-only gym access is enough to reply; the reply carries the trainer's email.
    let r = ok(
        &db,
        &trainer,
        "comment_reply",
        with_owner(&o, json!({"thread_id": thread, "body": "Yes, but slow the descent"})),
    )
    .await;
    let reply = &r["comments"][1];
    assert_eq!(
        (
            reply["author"].clone(),
            reply["by_owner"].clone(),
            reply["yours"].clone()
        ),
        (json!(trainer.email()), json!(false), json!(true))
    );
    let listed = ok(
        &db,
        &trainer,
        "comment_list",
        with_owner(&o, json!({"target": "set", "id": set})),
    )
    .await;
    assert_eq!(listed[0]["comments"].as_array().unwrap().len(), 2);
    // The trainer starts a thread of their own on the owner's exercise.
    ok(
        &db,
        &trainer,
        "thread_start",
        with_owner(
            &o,
            json!({"target": "exercise", "exercise": "Squat", "body": "Add pauses"}),
        ),
    )
    .await;

    // Vitals-only access and no access at all: refused.
    let e = err(&db, &doctor, "comment_list", with_owner(&o, json!({}))).await;
    assert!(e.contains("not shared their gym data"), "{e}");
    let e = err(
        &db,
        &stranger,
        "comment_reply",
        with_owner(&o, json!({"thread_id": thread, "body": "hi"})),
    )
    .await;
    assert!(e.contains("has not shared data with you"), "{e}");
    // Without owner, a thread id of someone else's logbook is not found.
    let e = err(
        &db,
        &trainer,
        "comment_reply",
        json!({"thread_id": thread, "body": "hi"}),
    )
    .await;
    assert!(e.contains("no comment thread"), "{e}");

    // sql_query sees the comments with gym access, not with vitals only.
    let rows = ok(
        &db,
        &trainer,
        "sql_query",
        with_owner(&o, json!({"sql": "SELECT count(*) AS n FROM comment"})),
    )
    .await;
    assert_eq!(rows["rows"], json!([{"n": 3}]));
    let e = err(
        &db,
        &doctor,
        "sql_query",
        with_owner(&o, json!({"sql": "SELECT count(*) FROM comment"})),
    )
    .await;
    assert!(e.contains("no access to the gym data"), "{e}");
}

/// Calls a tool through `/mcp` as `who`; returns whether it failed and its text, parsed as JSON when it is JSON.
async fn mcp(db: &health_mcp::db::Db, who: &Person, name: &str, args: Value) -> (bool, Value) {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}});
    let req = Request::post("/mcp")
        .header(header::HOST, "health.example.com")
        .header(header::AUTHORIZATION, format!("Bearer {}", token(MCP, who)))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app(db).oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    let out: Value = serde_json::from_slice(&body).unwrap();
    let result = &out["result"];
    let text = result["content"][0]["text"].as_str().unwrap_or_else(|| panic!("{out}"));
    let parsed = serde_json::from_str(text).unwrap_or_else(|_| json!(text));
    (result["isError"] == true, parsed)
}

/// The MCP path takes `owner` too: the trainer acts as themselves on the owner's logbook.
#[tokio::test]
async fn mcp_reply_and_delete_act_as_the_caller() {
    let db = db(None).await;
    let o = owner_with_data(&db).await;
    let (_, set) = ids(&db, &o).await;
    let trainer = person("trainer");
    share(&db, &o, &trainer, "ro", "none").await;
    let t = ok(
        &db,
        &o,
        "thread_start",
        json!({"target": "set", "id": set, "body": "Was this deep enough?"}),
    )
    .await;
    let thread = t["thread_id"].as_i64().unwrap();
    let owners = t["comments"][0]["id"].as_i64().unwrap();

    let (failed, r) = mcp(
        &db,
        &trainer,
        "comment_reply",
        with_owner(&o, json!({"thread_id": thread, "body": "Yes"})),
    )
    .await;
    assert!(!failed, "{r}");
    let reply = &r["comments"][1];
    assert_eq!(
        (
            reply["author"].clone(),
            reply["by_owner"].clone(),
            reply["yours"].clone()
        ),
        (json!(trainer.email()), json!(false), json!(true))
    );
    assert_eq!(r["comments"][0]["yours"], false);
    let (failed, e) = mcp(
        &db,
        &trainer,
        "comment_delete",
        with_owner(&o, json!({"comment_id": owners})),
    )
    .await;
    assert!(failed, "{e}");
    assert!(e.to_string().contains("only its author or the owner"), "{e}");
}

#[tokio::test]
async fn edit_delete_and_resolve_rules() {
    let db = db(None).await;
    let o = owner_with_data(&db).await;
    let (workout, _) = ids(&db, &o).await;
    let trainer = person("trainer");
    share(&db, &o, &trainer, "ro", "none").await;

    let t = ok(
        &db,
        &trainer,
        "thread_start",
        with_owner(&o, json!({"target": "workout", "id": workout, "body": "Good session"})),
    )
    .await;
    let thread = t["thread_id"].as_i64().unwrap();
    let theirs = t["comments"][0]["id"].as_i64().unwrap();
    let r = ok(
        &db,
        &o,
        "comment_reply",
        json!({"thread_id": thread, "body": "Thanks!"}),
    )
    .await;
    let mine = r["comments"][1]["id"].as_i64().unwrap();

    // Only the author edits.
    let e = err(
        &db,
        &o,
        "comment_update",
        json!({"comment_id": theirs, "body": "Bad session"}),
    )
    .await;
    assert!(e.contains("only its author"), "{e}");
    let u = ok(
        &db,
        &trainer,
        "comment_update",
        with_owner(&o, json!({"comment_id": theirs, "body": "Great session"})),
    )
    .await;
    assert_eq!(u["comments"][0]["body"], "Great session");
    assert!(u["comments"][0]["edited_at"].is_string());
    // The trainer can't delete the owner's comment; the owner can delete the trainer's.
    let e = err(
        &db,
        &trainer,
        "comment_delete",
        with_owner(&o, json!({"comment_id": mine})),
    )
    .await;
    assert!(e.contains("only its author or the owner"), "{e}");
    let d = ok(&db, &o, "comment_delete", json!({"comment_id": theirs})).await;
    assert_eq!(
        (d["comments"][0]["deleted"].clone(), d["comments"][0]["body"].clone()),
        (json!(true), Value::Null)
    );
    assert_eq!(d["comments"][1]["body"], "Thanks!", "replies keep their place");
    let e = err(
        &db,
        &trainer,
        "comment_update",
        with_owner(&o, json!({"comment_id": theirs, "body": "x"})),
    )
    .await;
    assert!(e.contains("no comment with id"), "{e}");

    // The starter resolves; resolved threads only show with include_resolved; a reply reopens.
    ok(
        &db,
        &trainer,
        "thread_resolve",
        with_owner(&o, json!({"thread_id": thread, "resolved": true})),
    )
    .await;
    assert_eq!(
        ok(&db, &o, "comment_list", json!({"target": "workout"})).await,
        json!([])
    );
    let all = ok(
        &db,
        &o,
        "comment_list",
        json!({"target": "workout", "include_resolved": true}),
    )
    .await;
    assert_eq!(all[0]["resolved"], true);
    let r = ok(
        &db,
        &o,
        "comment_reply",
        json!({"thread_id": thread, "body": "One more thing"}),
    )
    .await;
    assert_eq!(r["resolved"], false);
    // Someone who neither started the thread nor owns the logbook can't resolve it.
    let other = person("other");
    share(&db, &o, &other, "rw", "none").await;
    let e = err(
        &db,
        &other,
        "thread_resolve",
        with_owner(&o, json!({"thread_id": thread, "resolved": true})),
    )
    .await;
    assert!(e.contains("only whoever started"), "{e}");
}

#[tokio::test]
async fn can_resolve_follows_starter_and_owner() {
    let db = db(None).await;
    let o = owner_with_data(&db).await;
    let (workout, _) = ids(&db, &o).await;
    let (trainer, partner) = (person("trainer"), person("partner"));
    share(&db, &o, &trainer, "ro", "none").await;
    share(&db, &o, &partner, "ro", "none").await;

    let t = ok(
        &db,
        &trainer,
        "thread_start",
        with_owner(&o, json!({"target": "workout", "id": workout, "body": "Good session"})),
    )
    .await;
    assert_eq!(t["can_resolve"], true, "the starter");
    let args = json!({"target": "workout", "id": workout});
    let mine = ok(&db, &o, "comment_list", args.clone()).await;
    assert_eq!(mine[0]["can_resolve"], true, "the owner");
    let theirs = ok(&db, &partner, "comment_list", with_owner(&o, args)).await;
    assert_eq!(theirs[0]["can_resolve"], false, "another ro user");
}

#[tokio::test]
async fn workout_scope_includes_its_sets() {
    let db = db(None).await;
    let o = owner_with_data(&db).await;
    let (workout, set) = ids(&db, &o).await;
    let other = ok(
        &db,
        &o,
        "workout_add_past",
        json!({"gym": "Home", "started_at": "2026-10-03T18:00:00Z", "finished_at": "2026-10-03T19:00:00Z"}),
    )
    .await["workout_id"]
        .as_i64()
        .unwrap();
    let mut want = Vec::new();
    for (target, id) in [("workout", workout), ("set", set)] {
        let t = ok(
            &db,
            &o,
            "thread_start",
            json!({"target": target, "id": id, "body": "hi"}),
        )
        .await;
        want.push(t["thread_id"].clone());
    }
    ok(
        &db,
        &o,
        "thread_start",
        json!({"target": "workout", "id": other, "body": "elsewhere"}),
    )
    .await;
    ok(
        &db,
        &o,
        "thread_start",
        json!({"target": "exercise", "exercise": "Squat", "body": "elsewhere"}),
    )
    .await;

    let got = ok(&db, &o, "comment_list", json!({"workout_id": workout})).await;
    let mut got: Vec<Value> = got.as_array().unwrap().iter().map(|t| t["thread_id"].clone()).collect();
    got.reverse(); // newest first
    assert_eq!(got, want);
    let sets = ok(&db, &o, "comment_list", json!({"workout_id": workout, "target": "set"})).await;
    assert_eq!(sets.as_array().unwrap().len(), 1);
    let e = err(
        &db,
        &o,
        "comment_list",
        json!({"workout_id": workout, "target": "set", "id": set}),
    )
    .await;
    assert!(e.contains("not both"), "{e}");
}
