# vitals-mcp

A remote MCP server that stores and queries personal vitals (heart rate, HRV, body measurements such as weight, waist and blood pressure, and sleep) in TimescaleDB.
It supports multiple users, authenticates with OAuth (Authelia JWT access tokens) and offers typed tools plus a read-only SQL tool.
Raw tracker data (e.g. a Polar H10 chest strap) arrives through an ingest API (`POST /api/v1/ingest`),
from devices the app registers per user (`/api/v1/devices`).
Design and security model: [SPEC.md](SPEC.md).

## Run

```sh
docker build -t vitals-mcp .
docker run -p 8080:8080 \
  -e PUBLIC_URL=https://vitals.example.com \
  -e OIDC_ISSUER=https://auth.example.com \
  -e DATABASE_OWNER_URL=postgres://owner:…@db/vitals \
  -e DATABASE_URL=postgres://vitals_app:…@db/vitals \
  -e DATABASE_READER_URL=postgres://vitals_reader:…@db/vitals \
  vitals-mcp
```

`PUBLIC_URL` must be an origin without a path; the server refuses to start otherwise.
Probes: `/healthz` for liveness, `/readyz` for readiness (it checks both database pools).
On startup the server runs migrations as `DATABASE_OWNER_URL`. Leave that variable unset to skip them.
The owner needs `CREATEROLE` the first time, or you create the roles yourself beforehand:

```sql
CREATE ROLE vitals_app    LOGIN PASSWORD '…';
CREATE ROLE vitals_reader LOGIN PASSWORD '…';
-- importers: write to vitals_data.*, trusted to set user_id (the Authelia `sub`) and source
CREATE ROLE vitals_importer LOGIN PASSWORD '…';
GRANT USAGE ON SCHEMA vitals_data TO vitals_importer;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA vitals_data TO vitals_importer;
```

Don't give `vitals_reader` any grant on `vitals_data`: chunks would inherit it, and only the `vitals.*` views filter by user (see SPEC.md, Isolation).

**Authelia:** register a client with `access_token_signed_response_alg: RS256` and `audience: ['<PUBLIC_URL>/mcp']`
(full snippet in SPEC.md). Then add the connector in claude.ai with `<PUBLIC_URL>/mcp` and that client's ID and secret.
For the vitals-app phone app, register a second, public client `vitals-android` (PKCE, `offline_access`,
`audience: ['<PUBLIC_URL>/api']`, a long refresh-token lifespan; snippet in SPEC.md, Ingest API).
In the app, enter only `<PUBLIC_URL>`; it uploads to `/api/v1/ingest` and discovers the rest.

## Develop

```sh
docker compose up -d   # TimescaleDB on :55432 with dev roles
cargo test             # unit tests + DB integration and isolation tests
```

## Release

Same flow as gym-mcp. Conventional commits on `main` make release-plz open a release PR (version bump and CHANGELOG).
Merging it tags `vX.Y.Z`, which builds a multi-arch image (amd64 and arm64, native runners) and pushes it to
`ghcr.io/stephanmeijer/vitals-mcp` with build provenance. Actions are SHA-pinned in `.github/workflows/actions.lock`:
run `gh actions-lock` after changing any `uses:`.
