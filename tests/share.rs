//! Sharing, end to end through the real routes (`/api`, `/mcp`, `/invite`) with signed test tokens.
//! Needs the dev database: `docker compose up -d`.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use common::*;
use health_mcp::web::WebLogin;
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn invites_bind_to_the_verified_email() {
    let db = db(None).await;
    let (o, g, other) = (person("owner"), person("grantee"), person("other"));

    // Sharing needs a verified email on both sides.
    let no_email = Person {
        email: None,
        ..o.clone()
    };
    let e = err(&db, &no_email, "share_invite", json!({"email": g.email(), "gym": "ro"})).await;
    assert!(e.contains("verified email"), "{e}");
    let e = err(&db, &o, "share_invite", json!({"email": o.email(), "gym": "ro"})).await;
    assert!(e.contains("your own address"), "{e}");
    let e = err(&db, &o, "share_invite", json!({"email": g.email()})).await;
    assert!(e.contains("at least one"), "{e}");

    let (id, _) = invite(&db, &o, &g, "ro", "none").await;
    let sent = ok(&db, &o, "share_list", json!({})).await;
    assert_eq!(sent["invites_sent"][0]["email"], g.email());

    // Someone else, or the right person without a verified email, can't accept it.
    let e = err(&db, &other, "share_respond", json!({"invite_id": id, "accept": true})).await;
    assert!(e.contains("this invite is for"), "{e}");
    let unverified = Person {
        email: None,
        ..g.clone()
    };
    let e = err(
        &db,
        &unverified,
        "share_respond",
        json!({"invite_id": id, "accept": true}),
    )
    .await;
    assert!(e.contains("verified email"), "{e}");

    let out = ok(&db, &g, "share_respond", json!({"invite_id": id, "accept": true})).await;
    assert_eq!(out["owner"], o.email());
    let e = err(&db, &g, "share_respond", json!({"invite_id": id, "accept": true})).await;
    assert!(e.contains("no pending invite"), "{e}");

    let mine = ok(&db, &g, "share_list", json!({})).await;
    assert_eq!(
        mine["shared_with_you"],
        json!([{"owner": o.email(), "gym": "ro", "vitals": "none", "since": mine["shared_with_you"][0]["since"]}])
    );
    let theirs = ok(&db, &o, "share_list", json!({})).await;
    assert_eq!(theirs["shared_by_you"][0]["email"], g.email());
    assert_eq!(theirs["invites_sent"], json!([]));

    // Already shared: change it with share_update instead.
    let e = err(&db, &o, "share_invite", json!({"email": g.email(), "gym": "rw"})).await;
    assert!(e.contains("share_update"), "{e}");
}

#[tokio::test]
async fn expired_invites_cannot_be_accepted() {
    let db = db(None).await;
    let (o, g) = (person("owner"), person("grantee"));
    let (id, code) = invite(&db, &o, &g, "ro", "ro").await;
    sqlx::query("UPDATE share.invite SET expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(id)
        .execute(&db.app)
        .await
        .unwrap();
    let e = err(&db, &g, "share_respond", json!({"invite_id": id, "accept": true})).await;
    assert!(e.contains("expired"), "{e}");
    assert!(health_mcp::share::invite_by_code(&db, &code).await.unwrap().is_none());
}

#[tokio::test]
async fn access_follows_the_level_per_domain() {
    let db = db(None).await;
    let o = owner_with_data(&db).await;
    let g = person("grantee");
    let (id, _) = invite(&db, &o, &g, "ro", "none").await;
    ok(&db, &g, "share_respond", json!({"invite_id": id, "accept": true})).await;
    let as_owner = |mut args: Value| {
        args["owner"] = json!(o.email().to_uppercase());
        args
    };

    // Gym read-only: reads see the owner's data, writes are refused.
    let gyms = ok(&db, &g, "gym_list", as_owner(json!({}))).await;
    assert!(gyms.to_string().contains("Home"), "{gyms}");
    assert!(!ok(&db, &g, "gym_list", json!({})).await.to_string().contains("Home"));
    let e = err(&db, &g, "gym_create", as_owner(json!({"name": "Mine"}))).await;
    assert!(e.contains("read-only"), "{e}");
    let listed = ok(&db, &g, "workout_list", as_owner(json!({}))).await;
    let wid = &listed[0]["workout_id"];
    let got = ok(&db, &g, "workout_get", as_owner(json!({"workout_id": wid}))).await;
    assert_eq!(got, listed[0], "workout_get reads a shared logbook");
    let e = err(&db, &g, "workout_get", json!({"workout_id": wid})).await;
    assert!(
        e.contains("no workout with id"),
        "not in the grantee's own logbook: {e}"
    );
    // Vitals not shared: neither typed tools nor the SQL views.
    let e = err(&db, &g, "reading_list", as_owner(json!({"kind": "heart_rate"}))).await;
    assert!(e.contains("not shared their vitals"), "{e}");
    let rows = ok(
        &db,
        &g,
        "sql_query",
        as_owner(json!({"sql": "SELECT count(*) AS n FROM workout_set"})),
    )
    .await;
    assert_eq!(rows["rows"], json!([{"n": 1}]));
    let e = err(
        &db,
        &g,
        "sql_query",
        as_owner(json!({"sql": "SELECT count(*) FROM heart_rate"})),
    )
    .await;
    assert!(e.contains("no access to the vitals data"), "{e}");
    // Sharing tools and schema_describe never take owner; nor does a stranger get anything.
    let e = err(&db, &g, "share_list", as_owner(json!({}))).await;
    assert!(e.contains("only works on your own account"), "{e}");
    let stranger = person("stranger");
    let e = err(&db, &stranger, "gym_list", as_owner(json!({}))).await;
    assert!(e.contains("has not shared data with you"), "{e}");
    let e = err(&db, &g, "gym_list", json!({"owner": "nobody@example.com"})).await;
    assert!(e.contains("has not shared data with you"), "{e}");

    // Upgrade to rw on both: writes land in the owner's logbook, vitals become visible.
    ok(
        &db,
        &o,
        "share_update",
        json!({"email": g.email(), "gym": "rw", "vitals": "ro"}),
    )
    .await;
    ok(&db, &g, "gym_create", as_owner(json!({"name": "Office"}))).await;
    let gyms = ok(&db, &o, "gym_list", json!({})).await;
    assert!(gyms.to_string().contains("Office"), "{gyms}");
    let hr = ok(
        &db,
        &g,
        "reading_list",
        as_owner(json!({"kind": "heart_rate", "from": "2026-09-01T00:00:00Z"})),
    )
    .await;
    assert!(hr.to_string().contains("55"), "{hr}");
    let e = err(
        &db,
        &g,
        "reading_add",
        as_owner(json!({"kind": "heart_rate", "readings": [{"time": "2026-10-02T07:00:00Z", "bpm": 50}]})),
    )
    .await;
    assert!(e.contains("read-only"), "{e}");
    let rows = ok(
        &db,
        &g,
        "sql_query",
        as_owner(json!({"sql": "SELECT count(*) AS n FROM heart_rate"})),
    )
    .await;
    assert_eq!(rows["rows"], json!([{"n": 1}]));

    // Owner revokes: everything is refused again. Both none via share_update would do the same.
    let out = ok(&db, &o, "share_remove", json!({"email": g.email(), "what": "revoke"})).await;
    assert_eq!(out["removed"], "revoke");
    let e = err(&db, &g, "gym_list", as_owner(json!({}))).await;
    assert!(e.contains("has not shared data with you"), "{e}");
}

/// Each `what` removes exactly one direction or invite and errors when there is nothing of that kind.
#[tokio::test]
async fn share_remove_removes_one_thing() {
    let db = db(None).await;
    let (o, g) = (person("owner"), person("grantee"));
    // They share with each other.
    let (id, _) = invite(&db, &o, &g, "rw", "rw").await;
    ok(&db, &g, "share_respond", json!({"invite_id": id, "accept": true})).await;
    let (id, _) = invite(&db, &g, &o, "ro", "none").await;
    ok(&db, &o, "share_respond", json!({"invite_id": id, "accept": true})).await;
    let shared_by = |who| {
        let db = &db;
        async move { ok(db, who, "share_list", json!({})).await["shared_by_you"].clone() }
    };

    // g leaves o's data, but keeps sharing their own with o.
    let out = ok(&db, &g, "share_remove", json!({"email": o.email(), "what": "leave"})).await;
    assert_eq!(out["removed"], "leave");
    assert_eq!(shared_by(&o).await, json!([]));
    assert_eq!(shared_by(&g).await.as_array().unwrap().len(), 1);
    let e = err(&db, &g, "share_remove", json!({"email": o.email(), "what": "leave"})).await;
    assert!(e.contains("nothing is shared with you"), "{e}");

    // o invites g again, then withdraws it; g's share with o is untouched.
    invite(&db, &o, &g, "ro", "none").await;
    ok(&db, &o, "share_remove", json!({"email": g.email(), "what": "withdraw"})).await;
    assert_eq!(ok(&db, &o, "share_list", json!({})).await["invites_sent"], json!([]));
    assert_eq!(shared_by(&g).await.as_array().unwrap().len(), 1);
    let e = err(&db, &o, "share_remove", json!({"email": g.email(), "what": "withdraw"})).await;
    assert!(e.contains("no pending invite"), "{e}");
    let e = err(&db, &o, "share_remove", json!({"email": g.email(), "what": "revoke"})).await;
    assert!(e.contains("you don't share with"), "{e}");

    // g revokes their share; `what` is required.
    ok(&db, &g, "share_remove", json!({"email": o.email(), "what": "revoke"})).await;
    assert_eq!(shared_by(&g).await, json!([]));
    err(&db, &g, "share_remove", json!({"email": o.email()})).await;
}

#[tokio::test]
async fn grantee_can_decline() {
    let db = db(None).await;
    let (o, g) = (person("owner"), person("grantee"));
    let (id, _) = invite(&db, &o, &g, "ro", "none").await;
    ok(&db, &g, "share_respond", json!({"invite_id": id, "accept": false})).await;
    assert_eq!(ok(&db, &o, "share_list", json!({})).await["invites_sent"], json!([]));
}

/// The MCP path runs the same check in `call_tool`.
#[tokio::test]
async fn mcp_calls_take_owner_too() {
    let db = db(None).await;
    let o = owner_with_data(&db).await;
    let g = person("grantee");
    let (id, _) = invite(&db, &o, &g, "ro", "none").await;
    ok(&db, &g, "share_respond", json!({"invite_id": id, "accept": true})).await;

    let call = |name: &str, args: Value| {
        let body =
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}});
        Request::post("/mcp")
            .header(header::HOST, "health.example.com")
            .header(header::AUTHORIZATION, format!("Bearer {}", token(MCP, &g)))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let text = |res: axum::response::Response| async move {
        let body = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
        String::from_utf8(body.to_vec()).unwrap()
    };
    let res = app(&db)
        .oneshot(call("gym_list", json!({"owner": o.email()})))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let out = text(res).await;
    assert!(out.contains("Home") && !out.contains(r#""isError":true"#), "{out}");
    let res = app(&db)
        .oneshot(call(
            "set_add",
            json!({"owner": o.email(), "exercise": "Squat", "sets": [{"reps": 1}]}),
        ))
        .await
        .unwrap();
    let out = text(res).await;
    assert!(out.contains(r#""isError":true"#) && out.contains("read-only"), "{out}");
}

#[tokio::test]
async fn invite_page_logs_in_and_accepts() {
    let (o, g) = (person("owner"), person("grantee"));
    let login = WebLogin {
        client_id: "health-web".into(),
        client_secret: "s".into(),
        authorization_endpoint: "https://auth.example.com/api/oidc/authorization".into(),
        token_endpoint: token_endpoint(g.clone()).await,
        redirect_uri: format!("{PUBLIC}invite/callback"),
        audience: API.into(),
        http: reqwest::Client::new(),
    };
    let db = db(Some(login)).await;
    let (_, code) = invite(&db, &o, &g, "ro", "rw").await;
    let send = |req: Request<Body>| {
        let app = app(&db);
        async move {
            let res = app.oneshot(req).await.unwrap();
            let (status, headers) = (res.status(), res.headers().clone());
            let body = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
            (status, headers, String::from_utf8(body.to_vec()).unwrap())
        }
    };

    let (s, _, html) = send(Request::get(format!("/invite/{code}")).body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    assert!(html.contains(o.email()) && html.contains("read and write"), "{html}");
    let (s, _, _) = send(
        Request::get(format!("/invite/{}", "0".repeat(64)))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (s, h, _) = send(
        Request::post(format!("/invite/{code}/accept"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::SEE_OTHER);
    let location = reqwest::Url::parse(h[header::LOCATION].to_str().unwrap()).unwrap();
    let q: std::collections::HashMap<_, _> = location.query_pairs().into_owned().collect();
    assert_eq!(location.path(), "/api/oidc/authorization");
    assert_eq!(
        (q["client_id"].as_str(), q["code_challenge_method"].as_str()),
        ("health-web", "S256")
    );
    assert_eq!(q["audience"], API);
    let cookie = h[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let callback = |state: &str| {
        Request::get(format!("/invite/callback?code=the-code&state={state}"))
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap()
    };

    let (s, _, _) = send(callback(&"f".repeat(64))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "a state that isn't ours");
    let (s, h, html) = send(callback(&q["state"])).await;
    assert_eq!(s, StatusCode::OK, "{html}");
    assert!(html.contains("Invite accepted"), "{html}");
    assert!(h[header::SET_COOKIE].to_str().unwrap().contains("Max-Age=0"));
    let mine = ok(&db, &g, "share_list", json!({})).await;
    assert_eq!(mine["shared_with_you"][0]["vitals"], "rw");

    // Answered: the link is dead.
    let (s, _, _) = send(
        Request::post(format!("/invite/{code}/decline"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn declining_needs_only_the_link() {
    let db = db(None).await;
    let (o, g) = (person("owner"), person("grantee"));
    let (_, code) = invite(&db, &o, &g, "ro", "none").await;
    let res = app(&db)
        .oneshot(Request::get(format!("/invite/{code}")).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let html = String::from_utf8(axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap();
    assert!(
        html.contains("accept the invite there"),
        "no web login configured: {html}"
    );
    let res = app(&db)
        .oneshot(
            Request::post(format!("/invite/{code}/decline"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(ok(&db, &o, "share_list", json!({})).await["invites_sent"], json!([]));
}
