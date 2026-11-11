# health-mcp — v1 spec

A remote MCP server, written in Rust, that stores and queries personal health data in TimescaleDB: vitals and a gym
logbook (the former gym-mcp, see [Gym logbook](#gym-logbook)).
It supports multiple users. Each user sees only their own data: the untrusted SQL tool through signed-user views that
Postgres enforces, the typed tools and ingest through a `user_id` filter in every query (no row-level security).

## Scope

**In scope:** heart rate, HRV, body measurements (weight, body fat, height, waist, hip, neck, scale composition, SpO2,
temperature, glucose, blood pressure) and sleep. The gym logbook: gyms, exercises, workouts, sets, settings and body notes.
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

- health-mcp is a **resource server only**. It never issues tokens.
- It serves `/.well-known/oauth-protected-resource` (RFC 9728) with `authorization_servers: [<issuer>]`, the exact issuer
  named in `OIDC_ISSUER`'s discovery document (it may differ from `OIDC_ISSUER` by a trailing slash).
  It also advertises `scopes_supported: [openid, offline_access, email]`, so clients such as claude.ai request only those
  instead of every scope the IdP lists. Every Authelia client for health-mcp must allow these three scopes.
- Sharing needs the caller's verified email in the access token: `email` and `email_verified`, which Authelia puts in
  access tokens only through a claims policy (below). A token without them still works for everything except sharing.
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
        - client_id: 'claude-health'
          client_secret: '$pbkdf2-sha512$…'
          redirect_uris: ['https://claude.ai/api/mcp/auth_callback']
          scopes: ['openid', 'offline_access', 'email']
          grant_types: ['authorization_code', 'refresh_token']
          response_types: ['code']
          require_pkce: true
          pkce_challenge_method: 'S256'
          audience: ['https://health.example.com/mcp']    # = PUBLIC_URL + /mcp
          access_token_signed_response_alg: 'RS256'
          claims_policy: 'health'
      claims_policies:
        health:
          access_token: ['email', 'email_verified']
  ```

## Web sessions

The web app at `/app` (built from `web/`, served by `web_app`; `WEB_DIR`) logs the browser in with the `health-web`
client (below, Sharing) and keeps a server-side session. No tokens are stored.

1. `GET /app/login?next=/app/...` redirects to Authelia with PKCE. State, the verifier and `next` go in a 10-minute
   cookie `__Host-health_login` (HttpOnly, Secure, SameSite=Lax, path `/`), so a sibling subdomain can't plant its
   own login. `next` must be a printable-ASCII path inside `/app`; anything else becomes `/app/`.
2. `GET /app/callback` checks the state, exchanges the code and verifies the access token like an app token (aud
   `PUBLIC_URL/api`, `email` only when verified). It then creates a session in `web.session` (migration 0013).
   - The browser gets `__Host-session=<64 random hex>`, HttpOnly, Secure, SameSite=Lax, on path `/`; only its sha256 is
     stored.
   - A session ends 12 h after its last request (`last_seen`, refreshed at most once a minute) and 7 days after the
     login at the latest.
   - Expired rows are deleted on the next login.
3. `/api` accepts the cookie when there is no bearer token, and sets the same `User` and `VerifiedEmail` as a token
   would, so every tool, sharing and `owner` work unchanged.
   - With the cookie, a request other than GET/HEAD/OPTIONS must carry `Origin: PUBLIC_URL` and
     `X-Requested-With: health-web`, or it gets `403`. A cross-site form can't add that header, and a cross-site
     fetch can't without CORS, which the server doesn't allow.
   - Bearer requests are unchanged.
4. `POST /app/logout` (same `Origin` and header rule) deletes the session. `GET /app/me` returns `{sub, email,
   shared_with_you}` for the logbook switcher, or `401`.

Trade-off: a user disabled in Authelia keeps an existing web session until it ends (12 h idle, 7 days at most).
The session also keeps the email from the login for up to 7 days: if the address changes or stops being verified in
Authelia, the old address still drives sharing and `owner` resolution until the session ends. Deleting their rows in
`web.session` ends it at once.

Every `/app` response carries `Content-Security-Policy: frame-ancestors 'none'` and `X-Frame-Options: DENY`, so no
other page can frame the app. There is no full CSP: the shell's inline theme script would need a hash.

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
measurement    (time, user_id, source, device_id int → devices, type text, value real NOT NULL, unit text,
                note text, FOREIGN KEY (type, unit) → measurement_type (name, unit),
                UNIQUE NULLS NOT DISTINCT (user_id, source, device_id, type, time))   -- one row per value
measurement_type (name text PK, unit text, min real, max real)   -- plain table; a trigger keeps value in [min, max]

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
| hrv | `user_id, source` |
| measurement | `user_id, source, device_id, type` |
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
   `vitals_private.sign(…)` is `SECURITY DEFINER` and only `vitals_app` may execute it.
   For each `sql_query` the server sets the GUCs `vitals.user_id`, `vitals.domains` and `vitals.user_sig`
   (transaction-local; the signature covers domains and user, see Sharing).
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
Postgres first parses the bare query on its own (prepare, not execute), so `a; b`, `ROLLBACK; …` and any text that does
not parse as one statement are rejected; a statement that parses alone has balanced parentheses, quotes and comments,
so it cannot close the wrapper. It then runs as one statement inside `row_to_json(t.*) … FROM (\n…\n) t LIMIT 1001`
(so data-modifying statements and CTEs are rejected), with a running `sum(octet_length(…))` window around it that keeps
rows while their total is within 1 MiB, adds one `NULL` marker row if more was cut, and ends in a top-level `LIMIT 1001`.
At most 1000 rows and 1 MiB of row JSON come back (both caps run in Postgres, so a larger result never reaches the
server), then `ROLLBACK`. Either cap sets `truncated`.
The typed tools run as `vitals_app` with `statement_timeout = '30s'` per transaction (ingest too).

**Isolation tests** (`tests/db.rs`, all passing). User A tries to read or change user B's rows through each of these,
and every attempt fails: a forged `set_config`, `set_config('role')`, a direct chunk query, the base tables, `pg_stats`,
`vitals_private`, `ROLLBACK; …`, stacked statements, DELETE and a writable CTE, a 10 s `pg_sleep`, another user's
signature, and the write tools aimed at another user's row.

## MCP tools

The `kind` parameter takes `heart_rate` (field `bpm`), `hrv` (`rmssd_ms`), or a `measurement_type` name with field
`value` in that type's unit (migration 0008): `weight`, `muscle_mass`, `bone_mass` kg; `body_fat`, `body_water`, `spo2` %;
`height`, `waist`, `hip`, `neck` cm; `visceral_fat` level; `temperature` °C; `glucose` mmol/L; `systolic`,
`diastolic` mmHg; `pulse` bpm. Blood pressure is a `systolic` and a `diastolic` reading at the same time. A new
type is a row in `measurement_type` plus a `Kind` variant. Times are RFC 3339.
Tools that bucket by day also take `tz`, an IANA zone (default `UTC`).

| Tool | Purpose |
|---|---|
| `reading_add` | Add up to 1000 readings of one `kind`, always with `source = 'manual'`. |
| `reading_update` | Change the fields of the manual reading identified by (`kind`, `time`). |
| `reading_delete` | Delete manual readings by exact times, or by `kind` + time range. Returns the number deleted. |
| `reading_list` | Raw readings: `kind`, `from`, `to`, optional `source`, `limit` (≤1000), `oldest_first`. |
| `reading_aggregate` | `time_bucket(bucket, time, tz)` over `kind`/`field`, with `avg`, `min`, `max`, `count`, `stddev`, `p50`, `p90`, and optional `source`. At most 1000 buckets: `{"truncated": …, "buckets": […]}`. |
| `sleep_add` / `sleep_update` / `sleep_delete` | A session with optional stages. |
| `sleep_list` | Sessions in a range, with `include_stages` and per-stage totals: `{"truncated", "sessions"}`, at most 1000 sessions and, with `include_stages`, 1000 stage segments in total (per-stage totals stay complete). |
| `summary` | For each vital: the latest value plus 7-day and 30-day averages. Sleep: average duration over the last 7 and 30 days, and the stage split over the last 30 days (the same nights as `nights_30d`). |
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
          # gym-android: the same with client_id 'gym-android' and 'com.stephanmeijer.gym:/oauth/callback'
          scopes: ['openid', 'offline_access', 'email']
          claims_policy: 'health'
          grant_types: ['authorization_code', 'refresh_token']
          response_types: ['code']
          require_pkce: true
          pkce_challenge_method: 'S256'
          audience: ['https://health.example.com/api']    # = PUBLIC_URL + /api
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
  `sleep` stages are `awake`/`light`/`deep`/`rem`. A re-sent night with the same `start_at` is a no-op when it's identical;
  when its `end_at` or its stages differ, it replaces the stored night and its stages (the tracker may have finished its
  analysis since). Two versions of one night in the same batch: the last one wins.
  `recharge`'s last three fields are optional.
  `ecg` and `acc` take an optional `"encoding":"delta"` (default `"absolute"`): each array then holds differences from
  the previous value, the first from 0, e.g. `"samples":[-12,52,-33]` for `[-12,40,7]`. The server stores the rebuilt
  absolute values. Each difference and each rebuilt value must fit the array's type (`i16` for `acc`, `i32` for `ecg`),
  otherwise the frame is a `400`; send such a frame absolute. Deploy the server before an app that sends it.
  Unknown fields or types are errors. An empty body is a valid (heartbeat) batch.
  `ecg` and `acc` become one row per sample and `sleep` one row per stage, so a batch holds at most 200 000 of those
  rows together (about 10 minutes of H10 data; a real night has a few hundred stages). That commits in at most about
  8 s on the dev database (~33 µs per sample, ~39 µs per stage), well inside the 30 s statement
  timeout; a larger batch is a `413` instead of a timeout the app would retry forever. A single frame over the cap can't
  be split, so it is a `400` instead.
  Every frame time (`time`, `start_at`, `end_at`, stage `time`, and a recharge `night` by date, ±1 day for time
  zones) must lie between 8 days ago (the phone's 7-day outbox plus a day) and 1 hour from now; otherwise the frame is
  a `400`. This keeps a buggy clock from spreading one batch over thousands of hourly `ecg`/`acc` chunks.
  `device_id` is the id from `POST /api/v1/devices`; a frame with an id that isn't one of the user's devices is a `400`.
- **All or nothing:** one transaction with `ON CONFLICT DO NOTHING`, so a retried batch is a no-op.

| Response | Meaning |
|---|---|
| `200 {"frames":n,"inserted":m}` | Committed; `m` < `n` means some frames were already stored, or were `ecg`/`acc` frames with empty arrays |
| `400 {"frame":i,"error":"…"}` | Frame `i` (0-based) is invalid (including a time outside the window above, or over 200 000 samples or sleep stages on its own); nothing was stored |
| `401` | Missing or bad token, with the RFC 9728 challenge |
| `408` | Took more than 60 s, including waiting for a free slot; retry (if it did commit, the retry is a no-op) |
| `413` | Body over 32 MiB after decompression, more than 5000 frames, or more than 200 000 rows in total (`ecg` and `acc` samples and `sleep` stages); the app splits the batch and resends |
| `400 {"error":"…"}` | The database refused the batch for good (SQLSTATE class 22 or 23: a value or constraint the frame checks don't cover); nothing was stored. No frame index, so the app halves the batch to find the frame |
| `415` | Unsupported `Content-Type` or `Content-Encoding` |
| `503 {"error":"temporarily unavailable, retry"}` | Nothing ran (deadlock, restart, failover, or no free pool connection within 5 s); retry as is |
| `500` | Any other server-side failure, logged; retry |

Ingest writes as `vitals_app`, always with `source = 'app'` and the token's `user_id`. Before inserting, the server checks
every frame's `device_id` against the user's own devices (`WHERE user_id = $user`); app sleep stages only join app
sessions of that user. The MCP write tools only touch `source = 'manual'` rows, so they can't modify strap data.

### Devices

The app keeps the id per tracker: on connect it registers the tracker and uses the returned id in its frames.
Same auth as ingest.

- `GET /api/v1/devices` → `200 [{"id":1,"model":"Polar H10","serial":"244E7234"}]`, the user's devices.
- `POST /api/v1/devices` with `{"model":"Polar H10","serial":"244E7234"}` → `200 {"id":1}`. Idempotent: a known
  `(model, serial)` returns its existing id. Both fields 1–100 characters without NUL (Postgres text can't hold it;
  BLE device information is often NUL-padded), unknown fields are errors (`400`).

### Tools

The app calls the same tools as MCP clients, with the same arguments and results. Same auth as ingest.

- `GET /api/v1/tools` → `200 [{"name", "description", "inputSchema"}, …]`. The input JSON Schemas are generated from
  the same Rust structs as MCP's `tools/list`, so the app can generate its client types from them.
- `POST /api/v1/tools/{name}` with the tool's arguments as a JSON body (empty means `{}`) → `200` with the tool's
  output: JSON for reads (`reading_list`, `summary`, `sql_query`, …), a JSON string for `schema_describe`, and a small
  object for writes: `{"added": n, "kind"}`, `{"updated": 1, "kind", "time"}`, `{"deleted": n, "kind"}` for readings,
  `{"id"}` for `sleep_add`, `sleep_update` and `sleep_delete`.
  A tool error or bad argument → `400 {"error": "…"}` with the message the model would get; unknown tool → `404`;
  body over 1 MiB after decompression → `413`; a transient database failure → `503 {"error": "temporarily unavailable, retry"}`,
  safe to retry: SQLSTATE class 40 (deadlock, serialization failure; not 40002), 08000/08003/08006 reported by the
  server, 53300 too many connections, 53100 disk full, 57P01–57P03 restart or failover, or no free pool connection.
  A database error in a typed tool's own SQL is the caller's (`400`) only when the data was refused: SQLSTATE class 22
  (a bad value) or 23 (a CHECK, the reading range trigger, a key). Anything else there (a missing grant, an undefined
  column, a statement timeout) is the server's fault: logged with its SQLSTATE and answered `500` (`internal error`
  over MCP). A connection that drops mid-request is a `500` too, as is anything else unexpected.
  `sql_query` runs the caller's own SQL, so every database message from it (syntax, permission, its 5 s timeout) stays
  a `400`; its server-written setup (signing, `SET`s, rollback) is classified like a typed tool's.
- `sql_query` reads through the same signed `vitals.*` views as over MCP, so the app sees only the user's rows.
- The output of a tool has no schema beyond the description above; renaming a tool or an argument breaks the app as
  well as MCP clients.

## Gym logbook

The gym tools (`settings_*`, `gym_*`, `exercise_*`, `workout_*`, `set_*`, `body_note_*`, `training_overview`,
`muscle_group_volume`, `search`, `set_search`, `progress`, `personal_records`, `exercise_alternatives`) come from
gym-mcp unchanged, apart from its `measurement_*` tools, which are dropped (body fat and blood pressure are readings).
Added here for the web app's workout page: `workout_get` (one workout by id, in the shape of a `workout_list` row;
"no workout with id N" for an unknown or another user's id) and `body_note_list`'s `workout_id` filter.
Code: `src/gym/`. Usage rules for the model are in the server instructions (`src/gym/tools.rs`, `INSTRUCTIONS`).

- **Schema:** plain tables in schema `gym` (migration 0009, gym-mcp's schema after its migration 0019):
  `user_settings, gym, exercise, workout, workout_set, body_note`. Rows are keyed by `owner_sub`, the token's `sub`
  (the same value as `user_id` elsewhere); `workout_set` belongs to its workout's owner.
- **Access:** as `vitals_app`, with `search_path = gym, public` set per transaction (`src/gym/db.rs`), and like the
  vitals tools every query filters on `owner_sub` itself. Each transaction also sets the session `TimeZone` to the
  user's timezone, so local times, "today", weeks and months follow it. Read tools run in a `READ ONLY` transaction.
- **SQL:** migration 0010 adds `vitals.user_settings, gym, exercise, workout, workout_set, body_note` views for
  `vitals_reader`, filtered like the others (`workout_set` through a join on its workout); `vitals_reader` has no grant
  on schema `gym`. Tested in `sql_query_sees_only_the_callers_gym_rows`.
- **Sorting:** names, machines and variants sort with `COLLATE "und-x-icu"`. The TimescaleDB image is musl-based, and its
  `en_US.utf8` collation sorts bytewise ("Technogym" before "matrix").

### Comments

Flat comment threads about one **workout**, **exercise** or **set** (migration 0012): a thread is a first comment plus
replies in order, and a target can have several. `gym.comment_thread` binds to its target with exactly one of
`workout_id`, `exercise_id` and `set_id` (a CHECK ties it to `target`), each a foreign key with `ON DELETE CASCADE`: a
deleted set takes its threads with it. `gym.comment` holds the text, the author's `sub` and verified email, and
`edited_at`/`deleted_at`. The app can show each kind in its own place.

Access follows sharing (see Sharing):

| Who | Can |
|---|---|
| the owner of the logbook | everything below, on their own data, and delete any comment or resolve any thread on it |
| anyone the owner shares **gym** data with, `ro` or `rw` | read threads, start threads, reply (passing `owner`) |
| the author of a comment | edit and delete it |
| whoever started a thread | resolve or reopen it |

Read-only gym access is enough to comment on purpose: a trainer who only reads the logbook should still be able to give
feedback, and commenting doesn't change the logbook. `act_as` checks it through `Scope::Comment` (gym, at least `ro`),
whatever the tools' `read_only_hint`. Because `act_as` makes the owner the tool's user, it also passes the real caller
(`Author`) on, and comments are stored with that. A deleted comment keeps its place with an empty body; replying to a
resolved thread reopens it. Each thread carries `can_resolve` (the caller started it or owns the logbook), and
`comment_list{workout_id}` returns a workout's own threads plus those on its sets in one list (not combined with `id`;
with `target`, only that kind), so the app gets every set's threads in one call. Tools: `thread_start`, `comment_reply`, `comment_list`, `comment_update`, `comment_delete`,
`thread_resolve`. `sql_query` sees them through `vitals.comment_thread` and `vitals.comment`, filtered on the gym domain.

## Sharing

An owner lets another user read (`ro`) or read and write (`rw`) their data, per domain: `gym` (the logbook) and
`vitals`. Sharing itself, devices and ingest stay with the owner.

- **Invite:** `share_invite(email, gym, vitals)` stores an invite (schema `share`, migration 0011) for 7 days and mails
  a link `PUBLIC_URL/invite/<code>`. The code is 64 hex characters from the OS CSPRNG; only its sha256 is stored.
  Inviting the same address again replaces the invite. The owner must have a verified email, which the grantee sees.
- **Accept:** only a caller whose token carries the invited address as a verified email (`email_verified: true`) can
  accept: with `share_respond` (app or Claude, by the invite id from `share_list`), or on the invite page. The page
  shows who shares what, with Accept and Decline. Accept logs in with Authelia (authorization code with PKCE, client
  `health-web`, state and verifier in the 10-minute cookie `__Host-health_invite` (HttpOnly, Secure, SameSite=Lax, path
  `/`), so a sibling subdomain can't plant its own), gets an `/api` access token and checks it exactly like an app token
  before accepting. Decline needs only the link.
- **Use:** every data tool and `sql_query` take an optional `owner`: the owner's email (or `sub`). One check runs
  before the tool, for MCP (`call_tool`) and `/api` alike (`share::act_as`): the tool's domain comes from the router
  it is in, read or write from its `read_only_hint` (a tool without the hint counts as writing). On success the tool
  runs with the owner as its user and never sees the grantee. Unknown owner and "shares nothing with you" give the same
  error. `schema_describe` and the `share_*` tools refuse `owner`.
- **sql_query:** the reader transaction also sets `vitals.domains` (`gym,vitals`, or what the owner shared), and the
  signature covers `domains || '/' || user_id` (domains never contain `/`). `vitals.current_user_id()`, used by the vitals
  views, raises unless `vitals` is in the list; `vitals.gym_user_id()`, used by the gym views, unless `gym` is. A query
  can't widen its domains without breaking the signature.
- **Change and stop:** `share_update` (owner; both `none` removes the share), `share_remove` with a required `what`:
  `revoke` (stop sharing your data with `email`), `leave` (stop seeing `email`'s data) or `withdraw` (cancel your
  pending invite to `email`). Each removes exactly that one thing, so leaving someone's data keeps your own share with
  them, and errors when there is nothing of that kind. Access is checked on every call, so a change applies at once.

Authelia client for the invite page:

```yaml
        - client_id: 'health-web'
          client_secret: '$pbkdf2-sha512$…'
          redirect_uris: ['https://health.example.com/invite/callback', 'https://health.example.com/app/callback']
          scopes: ['openid', 'email']
          claims_policy: 'health'
          grant_types: ['authorization_code']
          response_types: ['code']
          require_pkce: true
          pkce_challenge_method: 'S256'
          audience: ['https://health.example.com/api']
          access_token_signed_response_alg: 'RS256'
          token_endpoint_auth_method: 'client_secret_basic'
```

## Config (env)

`DATABASE_URL` (vitals_app), `DATABASE_READER_URL` (vitals_reader), `DATABASE_OWNER_URL` (migrations only),
`OIDC_ISSUER` (a trailing slash is fine; tokens are checked against the issuer its discovery document names),
`PUBLIC_URL` (an origin without a path, e.g. `https://health.example.com`; this sets the resource/audience and the allowed `Host`;
the server refuses to start with a path),
`BIND_ADDR` (default `0.0.0.0:8080`), `RUST_LOG`, and for the gym tools `DEFAULT_TIMEZONE` (default `Europe/Amsterdam`)
and `WORKOUT_STALE_AFTER` (default `3h`, at most `8760h`).
Sharing: `SMTP_URL` (e.g. `smtp://user:pass@smtp.example.com:587?tls=required`; unset: no mail, `share_invite` returns
the link to pass on) with `MAIL_FROM`, and `OIDC_WEB_CLIENT_ID` / `OIDC_WEB_CLIENT_SECRET` (the `health-web` client;
unset: the invite page says to accept in the app or Claude).
Endpoints: `/mcp` (stateless streamable HTTP), `/api/v1/ingest`, `/invite/{code}` (public; the code is the secret), `/.well-known/oauth-protected-resource[/mcp|/api]`, `/healthz`
(liveness: always `200` while the process runs), `/readyz` (readiness: `200` when both database pools reach the database
within 2 s, else `503`; a pool whose connections are all busy counts as reachable, one check runs at a time per process,
and its result is reused for 1 s).

Limits (constants in the code):

- Database pools: 10 connections for `vitals_app`, 5 for `vitals_reader`. A request that waits more than 5 s for a
  connection fails as a transient error: `/mcp` tools return a "retry" tool error, `/api/v1/tools/{name}` returns
  `503`, and so do `/api/v1/ingest` and `/api/v1/devices` (the app retries both `503` and `500`).
- Requests: at most 16 `/mcp` and `/api` requests run at once across all users; more wait (bodies unread) until one
  finishes. A request that takes more than 60 s, waiting included, gets `408`. Discovery and health endpoints are exempt.

## Known limits

- Two transactions that each insert the first `sleep_stage` rows of a new time range (each creating a TimescaleDB chunk)
  can deadlock (`40P01`): chunk creation copies the foreign key to `sleep_session` and locks it. Postgres aborts one of them;
  importers should retry on `40P01`. Rare in practice: a chunk spans 7 days.
- No table has row-level security, so compression works everywhere. Isolation for `vitals_app` rests on the server
  filtering and setting `user_id` from the token; a server bug could read or write rows of another user.
- One invalid frame rejects its whole batch (`400`). A client that drops a batch on `400` loses its valid frames too.
  An unknown frame `type` is invalid too, so deploy a server that knows a new type before an app that sends it.
- App sleep is keyed on `start_at`: a changed night with the same start replaces the stored one, but if the tracker
  later reports a night with a different start, it's a second session.
- `pg_stat_activity` shows the *text* of other users' running `sql_query` calls (it's the same reader role), but not their data.
- Real Authelia tokens work in production for both clients: the claude.ai connector on `/mcp` and the vitals-android app
  on `/api`. That MCP tokens are rejected on `/api` and app tokens on `/mcp` is tested only with locally minted `at+jwt`
  tokens, not with real ones.
