# health-mcp

A remote MCP server for personal health, in TimescaleDB. It merges the former gym-mcp and vitals-mcp:

- **Gym logbook:** you say *"I arrived at the NDSM Trainmore"*, send a photo plus *"12 on 13kg"*, and it logs the workout. Ask *"what should I do next?"* and it recommends from your history and what that gym has.
- **Vitals:** heart rate, HRV, body measurements (weight, body fat, waist, blood pressure, glucose, …) and sleep, logged by hand or by trackers. Raw tracker data (e.g. a Polar H10 chest strap) arrives through an ingest API (`POST /api/v1/ingest`) from devices the app registers per user (`/api/v1/devices`).
- **One read-only SQL tool over both,** so a question like "training volume vs HRV" is one query.
- **Sharing:** invite someone by email to see (`ro`) or also change (`rw`) your gym logbook and/or vitals. They accept
  from the mail, in the app or in Claude, then pass your email as `owner` to any tool.

It supports multiple users, authenticates with OAuth (Authelia JWT access tokens) and keys all data by the token's `sub`.
Rust (rmcp, axum, sqlx). Design and security model: [SPEC.md](SPEC.md).

## Model

**Gym** (schema `gym`, plain tables):

| | |
|---|---|
| **Gym** | name + aliases, `equipment` (free text: "2 leg presses (max 300 kg), dumbbells 2–40 kg, no sled") and notes |
| **Exercise** | `kind` (strength or cardio), primary and secondary muscle groups from a fixed list (`back`, `shoulders`, `legs`, `arms` work as filters), `equipment` it needs (free text; empty = bodyweight) and a description |
| **Workout** | one visit to one gym; at most one open at a time |
| **Set** | reps / weight / duration; `set_type` (`working` by default, or `warmup`, `partial`, `interrupted`: only working sets count towards volume, progress, records and the last session); the `machine` (brand) and grip or `variant`, both defaulting to the last one used for that exercise at that gym; `unilateral` with an optional `load_basis`; optional `rpe` and `rest_s`; for cardio, distance, intensity and what the screen shows (heart rate, calories, incline, speed); `performed_at` is separate from `logged_at`, and `time_estimated` marks a guessed time |
| **Settings** | timezone, training `goal`, free-text `goals_notes`, `priority_muscle_groups` and `limitations` |
| **Body note** | a dated ache, pain or fatigue: `location`, optional `severity` 1–10, notes and the workout it came up in |
| **Comment thread** | a conversation about one workout, exercise or set: a first comment and replies, open or resolved. Written by the owner and anyone they share gym data with (`ro` is enough) |

**Vitals** (schema `vitals_data`, TimescaleDB hypertables): `heart_rate`, `hrv`, `measurement` (one value per row; body fat and blood pressure live here), `sleep_session`/`sleep_stage`, and raw tracker tables (`ecg`, `acc`, …). See SPEC.md, Schema.

**Gym time.** Each gym request runs with the Postgres session `TimeZone` set to the user's timezone (`settings_update`, default `DEFAULT_TIMEZONE`): local input like `2026-11-02 19:40` is read in that timezone, "today", ISO weeks and calendar months are local, and output is RFC 3339 with the local offset. The vitals tools take an explicit `tz` (default UTC) where they bucket by day.

**Workout lifecycle.** `workout_start` at the gym that already has the open workout resumes it; at a different gym it is refused unless you pass `finish_open: true`. A workout whose last activity is older than `WORKOUT_STALE_AFTER` is closed by the next workout action: at its last set, or deleted if it has no sets, notes or body notes. Past sessions: `workout_add_past`, then `set_add` with a `workout_id`.

## Tools

| Group | Tools |
|---|---|
| Gym setup | `settings_get/update`, `gym_create/update/get/list`, `exercise_create/update/search` |
| Workout | `workout_start/finish/update/current/add_past`, `workout_list`, `set_add/update/delete`, `body_note_add/list/update/delete` |
| Gym analysis | `training_overview`, `muscle_group_volume`, `search`, `set_search`, `progress`, `personal_records`, `exercise_alternatives` |
| Vitals | `reading_add/update/delete/list/aggregate`, `sleep_add/update/delete/list`, `summary` |
| SQL | `schema_describe`, `sql_query` (read-only, through per-user views over both gym and vitals data) |
| Comments | `thread_start`, `comment_reply`, `comment_list`, `comment_update`, `comment_delete`, `thread_resolve` |
| Sharing | `share_invite`, `share_list`, `share_update`, `share_remove`, `share_respond`; every data tool and `sql_query` take `owner` |

Wherever a gym tool takes a gym or exercise, you can give an id or a name, in any word order and with typos tolerated. An ambiguous or unknown name returns the candidates, so the model asks instead of guessing.

The same tools are served to apps as plain JSON: `GET /api/v1/tools`, `POST /api/v1/tools/{name}` (see SPEC.md, Ingest API → Tools).

## Run

```sh
docker build -t health-mcp .
docker run -p 8080:8080 \
  -e PUBLIC_URL=https://health.example.com \
  -e OIDC_ISSUER=https://auth.example.com \
  -e DATABASE_OWNER_URL=postgres://owner:…@db/health \
  -e DATABASE_URL=postgres://vitals_app:…@db/health \
  -e DATABASE_READER_URL=postgres://vitals_reader:…@db/health \
  health-mcp
```

| Env | |
|---|---|
| `PUBLIC_URL` | required; an origin without a path (the server refuses to start otherwise). MCP tokens need `aud` = `PUBLIC_URL/mcp`, app tokens `PUBLIC_URL/api` |
| `OIDC_ISSUER` | required; your Authelia URL |
| `DATABASE_URL` / `DATABASE_READER_URL` | required; as `vitals_app` (typed tools, ingest) and `vitals_reader` (`sql_query`) |
| `DATABASE_OWNER_URL` | runs the migrations on startup; leave unset to skip them |
| `DEFAULT_TIMEZONE` | gym timezone for users who haven't set one; default `Europe/Amsterdam` |
| `WORKOUT_STALE_AFTER` | default `3h` (`90m`, `45s` also work); at most `8760h` |
| `SMTP_URL`, `MAIL_FROM` | invite mails, e.g. `smtp://user:pass@smtp.example.com:587?tls=required`; unset: `share_invite` returns the link to pass on |
| `OIDC_WEB_CLIENT_ID`, `OIDC_WEB_CLIENT_SECRET` | the `health-web` client the invite page logs in with; unset: accept in the app or Claude |
| `WEB_DIR` | the built web app, served at `/app`; default `web/build` (`/web` in the image). Without it, `/app` is a 404 |
| `BIND_ADDR` | default `0.0.0.0:8080` |

Probes: `/healthz` for liveness, `/readyz` for readiness (it checks both database pools).
The owner needs `CREATEROLE` the first time, or you create the roles yourself beforehand:

```sql
CREATE ROLE vitals_app    LOGIN PASSWORD '…';
CREATE ROLE vitals_reader LOGIN PASSWORD '…';
-- importers: write to vitals_data.*, trusted to set user_id (the Authelia `sub`) and source
CREATE ROLE vitals_importer LOGIN PASSWORD '…';
GRANT USAGE ON SCHEMA vitals_data TO vitals_importer;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA vitals_data TO vitals_importer;
```

Don't give `vitals_reader` any grant on `vitals_data` or `gym`: chunks would inherit it, and only the `vitals.*` views filter by user (see SPEC.md, Isolation).

**Authelia:** register a client with `access_token_signed_response_alg: RS256` and `audience: ['<PUBLIC_URL>/mcp']`
(full snippet in SPEC.md). For sharing, every client needs the `email` scope and a claims policy that puts `email` and
`email_verified` in the access token, and the invite page needs a confidential client `health-web` (SPEC.md, Sharing). Then add the connector in claude.ai with `<PUBLIC_URL>/mcp` and that client's ID and secret.
For the phone apps, register public clients (`vitals-android`, `gym-android`) with PKCE, `offline_access` and
`audience: ['<PUBLIC_URL>/api']` (snippet in SPEC.md, Ingest API). In an app, enter only `<PUBLIC_URL>`; it discovers the rest.

### Moving from gym-mcp

Migration 0009 creates the gym tables (empty) in schema `gym`. Copy the data from the old gym database once:

```sh
pg_dump --data-only -t user_settings -t gym -t exercise -t workout -t workout_set -t body_note "$OLD_GYM_DB" \
  | sed '/^\(COPY\|SELECT pg_catalog.setval\)/s/public\./gym./' \
  | psql "$HEALTH_DB_OWNER"
```

pg_dump leaves out the generated search columns and resets the id sequences. gym-mcp's `measurement` table is not carried over.

### Upgrade and rollback

With `DATABASE_OWNER_URL` set, the server runs every pending migration in `migrations/` when it starts, before it
serves anything. There are no down migrations: once a new version has started, its migrations stay applied. An older
image with `DATABASE_OWNER_URL` set then refuses to start (`migration N was previously applied but is missing in the
resolved migrations`), and without it, it fails as soon as it touches a table that changed. Rolling back the image
alone is therefore not a rollback.

**Before upgrading** to a release that adds a file to `migrations/`, back up the whole database: a volume snapshot
with Postgres stopped, or a dump as the owner:

```sh
pg_dump -Fc -f health.dump "$HEALTH_DB_OWNER"
```

Dump the whole database, not single schemas. It must include `vitals_private.secret`, the key the server signs the
user id with for every `sql_query` (without it, every `sql_query` fails), and `_sqlx_migrations`, the record of applied
migrations (without it, the next start runs migration 0001 again and fails). `pg_dump` may print errors about
TimescaleDB's catalog; Timescale says these can usually be ignored. Roles are not in the dump: restore into a cluster
that already has the owner role (the user in `DATABASE_OWNER_URL`), `vitals_app`, `vitals_reader` and your importer
roles. The dump records that owner on every object, so later migrations, which run as the owner, can still change them.

**Rolling back:** stop the server, restore the backup, and start the previous image. Anything written after the backup
is lost for good, including what the phones uploaded: a phone deletes frames from its outbox once the server has
answered `200`, so only frames it never got a `200` for are sent again. Restore a dump into an empty database as a
superuser (`timescaledb_pre_restore()` needs one) and without `--no-owner`, so the objects stay owned by the owner
role. Follow Timescale's steps ([logical backup](https://www.tigerdata.com/docs/self-hosted/latest/backup-and-restore/logical-backup)):

```sh
psql "$EMPTY_DB_SUPERUSER" -c 'CREATE EXTENSION IF NOT EXISTS timescaledb' -c 'SELECT timescaledb_pre_restore()'
pg_restore -Fc -d "$EMPTY_DB_SUPERUSER" health.dump   # no -j: it doesn't restore TimescaleDB's catalog correctly
psql "$EMPTY_DB_SUPERUSER" -c 'SELECT timescaledb_post_restore()'
```

Restore on the same PostgreSQL and TimescaleDB versions you dumped from, then point the `DATABASE_*` URLs at that database.

**Skipping migrations:** leave `DATABASE_OWNER_URL` unset; the server then starts without touching the schema. It
fails at runtime if the schema is behind, so run the new version once with the owner URL when you're ready. Leaving it
unset is no way to run an older image on a newer schema: restore the backup instead.

**Migrations that change data for good** (the version that first shipped them in brackets):

- 0003 (0.2.0) replaces the text `device_id` of `ecg`, `acc`, `device_battery` and `device_event` with a number from
  the new `devices` table, and renames source `polar_h10` to `app`.
- 0006 (0.4.0) copies `ecg_frame` and `acc_frame` (the old one-row-per-frame data) into the per-sample `ecg` and `acc`
  tables, then drops them.
- 0008 (0.5.0) copies `body` into `measurement` and drops it, and drops `blood_pressure` without copying it.

## Develop

```sh
docker compose up -d   # TimescaleDB on :55432 with dev roles
DATABASE_URL=postgres://postgres:postgres@localhost:55432/health cargo test
```

The web app lives in `web/` (Svelte 5, TypeScript, shadcn-svelte); see [web/README.md](web/README.md).

`DATABASE_URL` is for the gym tests, which create a fresh database per test; the vitals tests use the dev database directly.

## Release

Conventional commits on `main` make release-plz open a release PR (version bump and CHANGELOG); versions continue from vitals-mcp's tags.
Merging it tags `vX.Y.Z`, which builds a multi-arch image (amd64 and arm64, native runners) and pushes it to
`ghcr.io/stephanmeijer/health-mcp` with build provenance. Actions are SHA-pinned in `.github/workflows/actions.lock`:
run `gh actions-lock` after changing any `uses:`.

A commit whose migration drops or rewrites data is a breaking change: write it as `feat(db)!: …` or add a
`BREAKING CHANGE:` footer. release-plz then marks it **breaking** in the CHANGELOG and bumps the minor version (while
below 1.0), so operators know to back up first.

## License

[MPL-2.0](LICENSE)
