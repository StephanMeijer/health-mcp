# vitals-mcp — v1 spec

A remote MCP server, written in Rust, that stores and queries personal vitals in TimescaleDB.
It supports multiple users. Each user sees only their own data: the untrusted SQL tool through signed-user views that
Postgres enforces, the typed tools and ingest through a `user_id` filter in every query (no row-level security).

## Scope

**In scope:** heart rate, HRV, blood pressure, body metrics (weight, body fat, SpO2, temperature) and sleep.
Manual logging and correction through MCP tools. Typed query tools and a read-only SQL tool.
An ingest API for raw tracker frames (e.g. Polar H10 via the vitals-app Android app), with a per-user device registry.
Auth with OAuth 2.1 as a resource server. A Dockerfile.

**Out of scope:** other importers (they write to the DB directly and are trusted to set `user_id` and `source` correctly),
the IdP itself, Kubernetes manifests, continuous aggregates, alerts and a UI.

## Stack

| Concern | Choice |
|---|---|
| MCP | `rmcp` with streamable HTTP transport, mounted in `axum` |
| DB | `sqlx` (Postgres) and `sqlx migrate`, run on startup |
| Auth | `jsonwebtoken` and a JWKS fetched from the issuer's discovery doc and cached |
| Image | multi-stage Dockerfile (`rust:1.99-slim-bookworm` → `distroless/cc-debian12:nonroot`), multi-arch via GitHub Actions, as in gym-mcp |

## Auth

- vitals-mcp is a **resource server only**. It never issues tokens.
- It serves `/.well-known/oauth-protected-resource` (RFC 9728) with `authorization_servers: [<issuer>]`, the exact issuer
  named in `OIDC_ISSUER`'s discovery document (it may differ from `OIDC_ISSUER` by a trailing slash).
  It also advertises `scopes_supported: [openid, offline_access]`, so clients such as claude.ai request only those
  instead of every scope the IdP lists. Every Authelia client for vitals-mcp must allow at least these two scopes.
- `/mcp` without a valid bearer token returns `401` with `WWW-Authenticate: Bearer resource_metadata="…"`.
- Token checks: signature (JWKS), `iss`, `aud` (= the resource, `PUBLIC_URL` + `/mcp`), `exp`, `nbf`. `user_id` = the token's `sub`.
- The metadata is also served at `/.well-known/oauth-protected-resource/mcp` (RFC 9728 path insertion).
- Validation is local only: no introspection call per request. The token header must have `typ: at+jwt`.
- **IdP: Authelia (latest, v4.39.x).** Checked against the v4.39.28 source:
  - It issues RFC 9068 JWT access tokens per client when `access_token_signed_response_alg` is set.
    The default is `none`, which gives opaque tokens.
  - `aud` comes from the client's `audience` list. Without one, the JWT has no `aud`
    (Authelia warns about this at startup).
  - With the default `requested_audience_mode: implicit`, the configured audience is granted even if the
    client doesn't request it. So claude.ai needs no `audience` parameter.
  - There's no Dynamic Client Registration (DCR) endpoint, so you register the client by hand and enter its ID and secret in the
    claude.ai connector's advanced settings.
  - `sub` is Authelia's opaque, stable UUID for each user.

  Client config:

  ```yaml
  identity_providers:
    oidc:
      clients:
        - client_id: 'claude-vitals'
          client_secret: '$pbkdf2-sha512$…'
          redirect_uris: ['https://claude.ai/api/mcp/auth_callback']
          scopes: ['openid', 'offline_access']
          grant_types: ['authorization_code', 'refresh_token']
          response_types: ['code']
          require_pkce: true
          pkce_challenge_method: 'S256'
          audience: ['https://vitals.example.com/mcp']    # = PUBLIC_URL + /mcp
          access_token_signed_response_alg: 'RS256'
  ```

## Schema

All times are `timestamptz`. Units are fixed in column names. `source` is the enum `vitals.source`: `manual` (MCP write tools), `app` (the ingest API), `import` (importers; they
describe what they read with a `devices` row, so new importers need no new enum value).
Base tables live in schema `vitals_data`; importers write there. Every vitals table is a hypertable and carries `user_id text NOT NULL` and `source vitals.source NOT NULL`, plus a
natural key `UNIQUE (user_id, source, time)`, so importers can write idempotently with `ON CONFLICT`. The exception is
`heart_rate`, keyed `UNIQUE NULLS NOT DISTINCT (user_id, source, device_id, time)`: target that with `ON CONFLICT`.

```sql
heart_rate     (time, user_id, source, bpm smallint NOT NULL, note text, rr_ms smallint[], device_id int → devices,
                UNIQUE NULLS NOT DISTINCT (user_id, source, device_id, time))
hrv            (time, user_id, source, rmssd_ms real NOT NULL, note text)
blood_pressure (time, user_id, source, systolic smallint NOT NULL, diastolic smallint NOT NULL,
                pulse smallint, note text)
body           (time, user_id, source, weight_kg real, fat_pct real, spo2_pct real, temp_c real, note text,
                CHECK (num_nonnulls(weight_kg, fat_pct, spo2_pct, temp_c) > 0))

sleep_session  (id uuid PK, user_id, source, start_at, end_at, note text, device_id int → devices,
                UNIQUE NULLS NOT DISTINCT (user_id, source, device_id, start_at), CHECK (end_at > start_at))   -- plain table
sleep_stage    (time, user_id, session_id → sleep_session ON DELETE CASCADE,
                stage text CHECK (stage IN ('awake','light','deep','rem')), duration_s int NOT NULL)  -- hypertable
```

```sql
devices        (id int identity PK, user_id, model text, serial text, created_at,   -- plain table
                UNIQUE (user_id, model, serial))       -- e.g. ('Polar H10', '244E7234')
```

Raw data from the ingest API (`source = 'app'`), keyed `UNIQUE (user_id, source, device_id, time)`. `ecg` and `acc`
store one row per sample: a frame's sample `i` (0-based) is at `time + i / sample_rate`, rounded to µs, so a resent
frame deduplicates sample by sample. A frame with an empty array stores nothing. Migration 0006 copied the data
stored one row per frame before 0005 into these tables (it took ~9 minutes for ~18.5 M samples, during startup).

```sql
ecg            (time, user_id, source, device_id int → devices, uv int)                      -- µV
acc            (time, user_id, source, device_id, x smallint, y smallint, z smallint)      -- milli-g
device_battery (time, user_id, source, level smallint, device_id)                             -- %
device_event   (time, user_id, source, event text, device_id)
skin_temp      (time, user_id, source, sample_rate smallint, celsius real[], device_id)   -- °C, CHECK 0 < c < 50
recharge       (night date, user_id, source, device_id, status smallint 1..6, ans_charge real -10..10,
                hrv_ms real, breathing_rate real, hr_bpm smallint,      -- plain table, one row per night
                UNIQUE (user_id, source, device_id, night))
```

`skin_temp` and `recharge` come from the Polar Loop. They are raw tables like the ones above; skin temperature is
~350 KB per day at 1 Hz before compression.
App sleep goes into `sleep_session`/`sleep_stage` with `source = 'app'` and the session's `device_id`, so `sleep_list` and
`summary` include it; the write tools still only touch manual sessions.

Strap heart rate goes into `heart_rate` (with `rr_ms`, the beat-to-beat intervals), so the MCP tools see it.
`heart_rate.device_id` is set for app rows and NULL for manual and imported ones, so two trackers of one user at the
same instant are two rows, while rows without a device still deduplicate on `(user_id, source, time)` (NULLs compare equal).
Rows from before migration 0003 (source `polar_h10`) have a `device_id` only if their user had exactly one device.

No table has row-level security (migration 0007 removed it): TimescaleDB refuses the columnstore on tables with RLS,
so every hypertable can now be compressed. `vitals_app` has only `INSERT` on the raw tables, and the server takes
`user_id` from the token. The reader keeps seeing only its own rows through the `vitals.*` views, which never depended
on RLS.

Every hypertable is compressed with `orderby time` and a `segmentby` equal to its unique key minus `time`, plus
`user_id` for sleep_stage (TimescaleDB needs the key columns to check uniqueness in compressed chunks); `ecg`/`acc` after 2 hours (below), the rest after
7 days, once the phone's outbox can no longer resend into them. Late resends and manual edits still work on compressed
chunks, more slowly.

| table | `compress_segmentby` |
|---|---|
| heart_rate, device_battery, device_event, skin_temp, ecg, acc | `user_id, source, device_id` |
| hrv, blood_pressure, body | `user_id, source` |
| sleep_stage | `user_id, session_id` |

`sleep_session`, `recharge` and `devices` are plain tables (not hypertables) and stay uncompressed; they're tiny.

`ecg` and `acc` use 1-hour chunks, compressed once older than 2 hours (`segmentby user_id, source, device_id`,
`orderby time`; `source` must be a segmentby column because it is in the unique key). A late resend from the phone's
7-day outbox lands in a compressed chunk and still deduplicates. Why one row per sample: TimescaleDB compresses
array columns generically, but scalar integer columns with delta-of-delta. Measured on 1.6 h of real H10 data:

| | per frame (arrays), raw → compressed | per sample, raw → compressed | full read, frame → sample |
|---|---|---|---|
| ecg | 5.0 → 1.63 MB | 101 → 1.41 MB | 75 → 23 ms |
| acc | 14.0 → 7.39 MB | 167 → 4.23 MB | 641 → 32 ms |

So about 85 MB per day at 24/7 instead of 135 MB, but ~15× larger until compressed, hence the short chunks.
lz4, 10-second arrays and delta-encoded arrays all saved less.

Each table also gets CHECK ranges for plausible values (for example `bpm BETWEEN 20 AND 250`).
These catch typos and unit mix-ups (lb vs kg, °F vs °C) at the boundary.

## Isolation

The SQL tool runs SQL from an LLM, so treat that SQL as untrusted (prompt injection).
A plain `SET vitals.user_id = sub` doesn't work, because any query can call `set_config()` and change it.
The design:

1. **Two login roles.** `vitals_app` handles the typed tools and ingest: SELECT, INSERT, UPDATE and DELETE on the
   manual tables, INSERT only on the raw tables. No row-level security.
   `vitals_reader` handles the SQL tool: SELECT on the views in schema `vitals` only. Neither role is a member of any
   other role, so `set_config('role', …)` can't escalate. Neither has `BYPASSRLS`. Migrations run as the owner role.
2. **Signed user id.** The migration creates a random secret in `vitals_private.secret`, which no login role can read.
   `vitals_private.sign(user_id)` is `SECURITY DEFINER` and only `vitals_app` may execute it.
   Per request the server sets the GUCs `vitals.user_id` and `vitals.user_sig` (transaction-local).
   `vitals.current_user_id()` (`SECURITY DEFINER`) checks the HMAC and raises an error if it doesn't match.
   A query can still set a different user id, but it can't produce a valid signature for it.
3. **`vitals_app`:** every query filters on `user_id = $user` itself (the user from the verified token), and writes
   also on `source = 'manual'`. The database doesn't enforce this, so a query without that filter would leak or change
   another user's rows. `tests/db.rs` guards it: `other_users_rows_are_invisible_and_untouchable` runs every typed tool
   as a second user, and those tests fail when a filter is removed.
4. **`vitals_reader` gets views, never base tables.** Each view is `vitals.<table>`, created `WITH (security_barrier)`
   and filtered on `user_id = vitals.current_user_id()`.
   Reason (found in testing): TimescaleDB chunks copy the hypertable's *grants*. A reader with SELECT on the
   hypertable could therefore read every user's rows straight from `_timescaledb_internal._hyper_*_chunk`.
   Revoking the TimescaleDB schemas isn't an option, because hypertable queries stop working.
   With no grant on the base tables, chunks and `pg_stats` (which only lists readable columns) show the reader nothing.

`sql_query` runs like this: `BEGIN`, `SET TRANSACTION READ ONLY`, `SET LOCAL statement_timeout = '5s'`, then the signed GUCs.
The query runs as one prepared statement wrapped in `SELECT row_to_json(t.*) FROM (…) t LIMIT 1001`
(so `a; b`, `ROLLBACK; …` and data-modifying CTEs are rejected). At most 1000 rows come back, then `ROLLBACK`.
The typed tools run as `vitals_app` with `statement_timeout = '30s'` per transaction (ingest too).

**Isolation tests** (`tests/db.rs`, all passing). User A tries to read or change user B's rows through each of these,
and every attempt fails: a forged `set_config`, `set_config('role')`, a direct chunk query, the base tables, `pg_stats`,
`vitals_private`, `ROLLBACK; …`, stacked statements, DELETE and a writable CTE, a 10 s `pg_sleep`, another user's
signature, and the write tools aimed at another user's row.

## MCP tools

The `kind` parameter takes one of `heart_rate | hrv | blood_pressure | body`. Times are RFC 3339.
Tools that bucket by day also take `tz`, an IANA zone (default `UTC`).

| Tool | Purpose |
|---|---|
| `reading_add` | Add up to 1000 readings of one `kind`, always with `source = 'manual'`. |
| `reading_update` | Change the fields of the manual reading identified by (`kind`, `time`). |
| `reading_delete` | Delete manual readings by exact times, or by `kind` + time range. Returns the number deleted. |
| `reading_list` | Raw readings: `kind`, `from`, `to`, optional `source`, `limit` (≤1000), `oldest_first`. |
| `reading_aggregate` | `time_bucket(bucket, time, tz)` over `kind`/`field`, with `avg`, `min`, `max`, `count`, `stddev`, `p50`, `p90`, and optional `source`. At most 1000 buckets: `{"truncated": …, "buckets": […]}`. |
| `sleep_add` / `sleep_update` / `sleep_delete` | A session with optional stages. |
| `sleep_list` | Sessions in a range, with `include_stages` and per-stage totals. |
| `summary` | For each vital: the latest value plus 7-day and 30-day averages. Sleep: average duration and stage split over the last 7 and 30 days. |
| `schema_describe` | Tables, columns, units and example queries, so the model can write correct SQL. |
| `sql_query` | Read-only SQL as described above. Returns rows as JSON objects (column order kept) and a `truncated` flag. |

**Write tools touch only `source = 'manual'` rows.** `*_add` always writes `source = 'manual'` (there's no `source` parameter).
`*_update` and `*_delete` add `AND source = 'manual'` to their SQL, and return "not found" for imported rows.
This is in the SQL of each tool; the database doesn't enforce it (no row-level security).
Importers connect as their own role and write to `vitals_data.*` (they're trusted to set `user_id` and `source`).

## Ingest API

`POST /api/v1/ingest` stores a batch of device frames. The phone keeps frames in a local outbox and deletes them only after a `200`.

- **Auth:** OAuth like `/mcp`, but its own resource: `aud` must be `PUBLIC_URL` + `/api`, so an MCP token is rejected here
  and vice versa. Metadata: `/.well-known/oauth-protected-resource/api`. `user_id` = `sub`, never taken from the body.
  The app needs only the base URL: metadata → `authorization_servers` → the issuer's OIDC discovery. Authelia client:
  ```yaml
        - client_id: 'vitals-android'           # convention the app uses
          public: true
          token_endpoint_auth_method: 'none'
          redirect_uris: ['com.stephanmeijer.vitals:/oauth/callback']   # must match the app's redirect scheme
          scopes: ['openid', 'offline_access']
          grant_types: ['authorization_code', 'refresh_token']
          response_types: ['code']
          require_pkce: true
          pkce_challenge_method: 'S256'
          audience: ['https://vitals.example.com/api']    # = PUBLIC_URL + /api
          access_token_signed_response_alg: 'RS256'
          consent_mode: 'pre-configured'
          lifespan: 'vitals-device'   # define under identity_providers.oidc.lifespans.custom with a long refresh_token
  ```
- **Body:** `Content-Type: application/x-ndjson` (one frame per line, empty lines ignored) or `application/cbor-seq`
  (RFC 8742). Optional `Content-Encoding: zstd` or `gzip`. Frames, tagged by `type`, times RFC 3339 (µs precision):
  ```jsonl
  {"type":"hr","device_id":1,"time":"2026-11-04T10:00:00.123456Z","bpm":72,"rr_ms":[812,830]}
  {"type":"ecg","device_id":1,"time":"…","sample_rate":130,"samples":[-12,40]}
  {"type":"acc","device_id":1,"time":"…","sample_rate":200,"x":[1],"y":[2],"z":[-1000]}
  {"type":"battery","device_id":1,"time":"…","level":87}
  {"type":"status","device_id":1,"time":"…","event":"connected"}
  {"type":"skin_temp","device_id":2,"time":"…","sample_rate":1,"celsius":[33.25,33.5]}
  {"type":"sleep","device_id":2,"start_at":"…","end_at":"…","stages":[{"time":"…","stage":"light","duration_s":600}]}
  {"type":"recharge","device_id":2,"night":"2026-11-04","status":4,"ans_charge":2.5,"hrv_ms":48,"breathing_rate":14.5,"hr_bpm":52}
  ```
  `sleep` stages are `awake`/`light`/`deep`/`rem`. A re-sent night with the same `start_at` is a no-op, including its stages.
  `recharge`'s last three fields are optional.
  `ecg` and `acc` take an optional `"encoding":"delta"` (default `"absolute"`): each array then holds differences from
  the previous value, the first from 0, e.g. `"samples":[-12,52,-33]` for `[-12,40,7]`. The server stores the rebuilt
  absolute values. Each difference and each rebuilt value must fit the array's type (`i16` for `acc`, `i32` for `ecg`),
  otherwise the frame is a `400`; send such a frame absolute. Deploy the server before an app that sends it.
  Unknown fields or types are errors. An empty body is a valid (heartbeat) batch.
  `device_id` is the id from `POST /api/v1/devices`; a frame with an id that isn't one of the user's devices is a `400`.
- **All or nothing:** one transaction with `ON CONFLICT DO NOTHING`, so a retried batch is a no-op.

| Response | Meaning |
|---|---|
| `200 {"frames":n,"inserted":m}` | Committed; `m` < `n` means some frames were already stored, or were `ecg`/`acc` frames with empty arrays |
| `400 {"frame":i,"error":"…"}` | Frame `i` (0-based) is invalid; nothing was stored |
| `401` | Missing or bad token, with the RFC 9728 challenge |
| `413` | Body over 32 MiB after decompression, or more than 5000 frames |
| `415` | Unsupported `Content-Type` or `Content-Encoding` |
| `500` | Database error; retry |

Ingest writes as `vitals_app`, always with `source = 'app'` and the token's `user_id`. Before inserting, the server checks
every frame's `device_id` against the user's own devices (`WHERE user_id = $user`); app sleep stages only join app
sessions of that user. The MCP write tools only touch `source = 'manual'` rows, so they can't modify strap data.

### Devices

The app keeps the id per tracker: on connect it registers the tracker and uses the returned id in its frames.
Same auth as ingest.

- `GET /api/v1/devices` → `200 [{"id":1,"model":"Polar H10","serial":"244E7234"}]`, the user's devices.
- `POST /api/v1/devices` with `{"model":"Polar H10","serial":"244E7234"}` → `200 {"id":1}`. Idempotent: a known
  `(model, serial)` returns its existing id. Both fields 1–100 characters, unknown fields are errors (`400`).

### Tools

The app calls the same tools as MCP clients, with the same arguments and results. Same auth as ingest.

- `GET /api/v1/tools` → `200 [{"name", "description", "inputSchema"}, …]`. The input JSON Schemas are generated from
  the same Rust structs as MCP's `tools/list`, so the app can generate its client types from them.
- `POST /api/v1/tools/{name}` with the tool's arguments as a JSON body (empty means `{}`) → `200` with the tool's
  output: JSON for reads (`reading_list`, `summary`, `sql_query`, …), a JSON string for write messages
  (`"added 1 heart_rate reading(s)"`).
  A tool error or bad argument → `400 {"error": "…"}` with the message the model would get; unknown tool → `404`;
  body over 1 MiB after decompression → `413`; a transient database failure (SQLSTATE class 40: deadlock,
  serialization failure) → `503 {"error": "temporarily unavailable, retry"}`, safe to retry; internal error → `500`.
  A `sql_query` statement timeout stays a `400`.
- `sql_query` reads through the same signed `vitals.*` views as over MCP, so the app sees only the user's rows.
- The output of a tool has no schema beyond the description above; renaming a tool or an argument breaks the app as
  well as MCP clients.

## Config (env)

`DATABASE_URL` (vitals_app), `DATABASE_READER_URL` (vitals_reader), `DATABASE_OWNER_URL` (migrations only),
`OIDC_ISSUER` (a trailing slash is fine; tokens are checked against the issuer its discovery document names),
`PUBLIC_URL` (an origin without a path, e.g. `https://vitals.example.com`; this sets the resource/audience and the allowed `Host`;
the server refuses to start with a path),
`BIND_ADDR` (default `0.0.0.0:8080`), `RUST_LOG`.
Endpoints: `/mcp` (stateless streamable HTTP), `/api/v1/ingest`, `/.well-known/oauth-protected-resource[/mcp|/api]`, `/healthz`.

## Known limits

- Two transactions that each insert the first `sleep_stage` rows of a new time range (each creating a TimescaleDB chunk)
  can deadlock (`40P01`): chunk creation copies the foreign key to `sleep_session` and locks it. Postgres aborts one of them;
  importers should retry on `40P01`. Rare in practice: a chunk spans 7 days.
- No table has row-level security, so compression works everywhere. Isolation for `vitals_app` rests on the server
  filtering and setting `user_id` from the token; a server bug could read or write rows of another user.
- One invalid frame rejects its whole batch (`400`). A client that drops a batch on `400` loses its valid frames too.
  An unknown frame `type` is invalid too, so deploy a server that knows a new type before an app that sends it.
- App sleep deduplicates on `start_at`: if the tracker later reports a night with a different start, it's a second session.
- `pg_stat_activity` shows the *text* of other users' running `sql_query` calls (it's the same reader role), but not their data.
- Not tested against a real Authelia token or the claude.ai connector flow; only a locally minted `at+jwt` was used.
