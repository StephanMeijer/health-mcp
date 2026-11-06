# vitals-mcp — v1 spec

A remote MCP server, written in Rust, that stores and queries personal vitals in TimescaleDB.
It supports multiple users. Each user sees only their own data, and Postgres enforces that (RLS plus signed-user views).

## Scope

**In scope:** heart rate, HRV, blood pressure, body metrics (weight, body fat, SpO2, temperature) and sleep.
Manual logging and correction through MCP tools. Typed query tools and a read-only SQL tool.
An ingest API for raw tracker frames (e.g. Polar H10 via the vitals-app Android app), with a per-user device registry.
Auth with OAuth 2.1 as a resource server. A Dockerfile.

**Out of scope:** other importers (they write to the DB directly and are trusted to set `user_id` and `source` correctly),
the IdP itself, Kubernetes manifests, continuous aggregates (they don't support RLS), alerts and a UI.

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

Raw frames from the ingest API (`source = 'app'`), one row per frame as the device sent it, keyed
`UNIQUE (user_id, source, device_id, time)`. `time` is the first sample; sample `i` (0-based) is at `time + i / sample_rate`.

```sql
ecg            (time, user_id, source, sample_rate smallint, samples int[], device_id int → devices)        -- µV
acc            (time, user_id, source, sample_rate smallint, x smallint[], y smallint[], z smallint[], device_id)  -- milli-g
device_battery (time, user_id, source, level smallint, device_id)                             -- %
device_event   (time, user_id, source, event text, device_id)
skin_temp      (time, user_id, source, sample_rate smallint, celsius real[], device_id)   -- °C, CHECK 0 < c < 50
recharge       (night date, user_id, source, device_id, status smallint 1..6, ans_charge real -10..10,
                hrv_ms real, breathing_rate real, hr_bpm smallint,      -- plain table, one row per night
                UNIQUE (user_id, source, device_id, night))
```

`skin_temp` and `recharge` come from the Polar Loop. They are raw tables like the ones above, but neither is compressed: skin temperature is ~350 KB per day at 1 Hz.
App sleep goes into `sleep_session`/`sleep_stage` with `source = 'app'` and the session's `device_id`, so `sleep_list` and
`summary` include it; the write tools still only touch manual sessions.

Strap heart rate goes into `heart_rate` (with `rr_ms`, the beat-to-beat intervals), so the MCP tools see it.
`heart_rate.device_id` is set for app rows and NULL for manual and imported ones, so two trackers of one user at the
same instant are two rows, while rows without a device still deduplicate on `(user_id, source, time)` (NULLs compare equal).
Rows from before migration 0003 (source `polar_h10`) have a `device_id` only if their user had exactly one device.

The four raw tables have **no RLS**, so that `ecg` and `acc` can be compressed (TimescaleDB refuses the columnstore on tables
with row security). Chunks older than 8 days are compressed, after the phone's 7-day outbox can no longer resend into them;
a late resend still works and still deduplicates. `vitals_app` has only `INSERT` on them, and the server takes `user_id`
from the token. The reader keeps seeing only its own rows through the `vitals.*` views, which don't depend on RLS.
Measured on 1.87 h of real strap data: ecg 6.0 MB → 1.9 MB, acc 18 MB → 7.7 MB, heart_rate 1.8 MB (not compressed),
so about 6 MB per hour of wear, ~150 MB per day at 24/7. One row per sample compresses better (ecg 1.8 MB, acc 4.8 MB)
but is ~10× larger until compressed; delta-encoding the arrays saved only 5–16 %. Neither is worth it.

Each table also gets CHECK ranges for plausible values (for example `bpm BETWEEN 20 AND 250`).
These catch typos and unit mix-ups (lb vs kg, °F vs °C) at the boundary.

## Isolation

The SQL tool runs SQL from an LLM, so treat that SQL as untrusted (prompt injection).
A plain `SET vitals.user_id = sub` doesn't work, because any query can call `set_config()` and change it.
The design:

1. **Two login roles.** `vitals_app` handles the typed tools: SELECT, INSERT, UPDATE and DELETE on `vitals_data.*`, under RLS.
   `vitals_reader` handles the SQL tool: SELECT on the views in schema `vitals` only. Neither role is a member of any
   other role, so `set_config('role', …)` can't escalate. Neither has `BYPASSRLS`. Migrations run as the owner role.
2. **Signed user id.** The migration creates a random secret in `vitals_private.secret`, which no login role can read.
   `vitals_private.sign(user_id)` is `SECURITY DEFINER` and only `vitals_app` may execute it.
   Per request the server sets the GUCs `vitals.user_id` and `vitals.user_sig` (transaction-local).
   `vitals.current_user_id()` (`SECURITY DEFINER`) checks the HMAC and raises an error if it doesn't match.
   A query can still set a different user id, but it can't produce a valid signature for it.
3. **`vitals_app`:** RLS policies on `vitals_data.*` with `user_id = vitals.current_user_id()`.
4. **`vitals_reader` gets views, never base tables.** Each view is `vitals.<table>`, created `WITH (security_barrier)`
   and filtered on `user_id = vitals.current_user_id()`.
   Reason (found in testing): TimescaleDB chunks copy the hypertable's *grants* but not its *RLS*. A reader with SELECT
   on the hypertable could therefore read every user's rows straight from `_timescaledb_internal._hyper_*_chunk`.
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
| `reading_add` | Add one or more readings of one `kind`, always with `source = 'manual'`. |
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
This is enforced in the SQL itself, not only in tool validation: the RLS write policies (`INSERT`, `UPDATE` and `DELETE` for
`vitals_app`) carry `WITH CHECK / USING (source = 'manual')`, so a server bug can't change imported data either.
Importers connect as their own role and write to `vitals_data.*`. RLS denies a role that matches no policy, so give
importer roles `BYPASSRLS` (they're trusted to set `user_id` and `source`).

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
  absolute values; one that doesn't fit its type (`i16` for `acc`) is a `400`. Deploy the server before an app that sends it.
  Unknown fields or types are errors. An empty body is a valid (heartbeat) batch.
  `device_id` is the id from `POST /api/v1/devices`; a frame with an id that isn't one of the user's devices is a `400`.
- **All or nothing:** one transaction with `ON CONFLICT DO NOTHING`, so a retried batch is a no-op.

| Response | Meaning |
|---|---|
| `200 {"frames":n,"inserted":m}` | Committed; `m` < `n` means some frames were already stored |
| `400 {"frame":i,"error":"…"}` | Frame `i` (0-based) is invalid; nothing was stored |
| `401` | Missing or bad token, with the RFC 9728 challenge |
| `413` | Body over 32 MiB after decompression, or more than 5000 frames |
| `415` | Unsupported `Content-Type` or `Content-Encoding` |
| `500` | Database error; retry |

Ingest writes as `vitals_app`. On `heart_rate` and `sleep_session` the `ingest_insert` RLS policy allows only `INSERT` of `source = 'app'`
rows for the signed user and one of their devices (on `sleep_stage`: only into an app session); the `manual_write` policies are unchanged, so MCP tools still can't modify strap data.
The raw tables rely on the server instead (see Schema): it checks every frame's `device_id` against the user's devices.

### Devices

The app keeps the id per tracker: on connect it registers the tracker and uses the returned id in its frames.
Same auth as ingest.

- `GET /api/v1/devices` → `200 [{"id":1,"model":"Polar H10","serial":"244E7234"}]`, the user's devices.
- `POST /api/v1/devices` with `{"model":"Polar H10","serial":"244E7234"}` → `200 {"id":1}`. Idempotent: a known
  `(model, serial)` returns its existing id. Both fields 1–100 characters, unknown fields are errors (`400`).

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
- The raw tables (`ecg`, `acc`, `device_battery`, `device_event`, `skin_temp`, `recharge`) have no RLS, so compression works. Isolation
  there rests on the server setting `user_id` from the token; a server bug could write rows under another user.
- One invalid frame rejects its whole batch (`400`). A client that drops a batch on `400` loses its valid frames too.
  An unknown frame `type` is invalid too, so deploy a server that knows a new type before an app that sends it.
- App sleep deduplicates on `start_at`: if the tracker later reports a night with a different start, it's a second session.
- `pg_stat_activity` shows the *text* of other users' running `sql_query` calls (it's the same reader role), but not their data.
- Not tested against a real Authelia token or the claude.ai connector flow; only a locally minted `at+jwt` was used.
