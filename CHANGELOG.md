# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.1](https://github.com/StephanMeijer/health-mcp/compare/v0.6.0...v0.6.1) - 2026-11-10

### Fixed

- *(mcp)* serve the gym tools again; tool_handler used the vitals router only

## [0.6.0](https://github.com/StephanMeijer/health-mcp/compare/v0.5.1...v0.6.0) - 2026-11-10

### Added

- [**breaking**] merge gym-mcp into vitals-mcp as health-mcp: the gym logbook tools, schema `gym` and per-user gym views for `sql_query`. New crate, image (`ghcr.io/stephanmeijer/health-mcp`) and host; see the README for moving the gym data

### Other

- document the merged health-mcp
- merge gym-mcp history

## [0.5.1](https://github.com/StephanMeijer/vitals-mcp/compare/v0.5.0...v0.5.1) - 2026-11-09

### Other

- run on GitHub-hosted runners (free for public repos) ([#83](https://github.com/StephanMeijer/vitals-mcp/pull/83))
- verify the actions lockfile on the 9xx runners ([#81](https://github.com/StephanMeijer/vitals-mcp/pull/81))

## [0.5.0](https://github.com/StephanMeijer/vitals-mcp/compare/v0.4.3...v0.5.0) - 2026-11-06

### Added

- *(tools)* [**breaking**] store body measurements one value per row
- *(api)* cap ecg and acc samples per ingest batch
- *(api)* reject ingest frames outside a plausible time window
- *(tools)* cap sql_query output at 1 MiB ([#75](https://github.com/StephanMeijer/vitals-mcp/pull/75))
- *(tools)* [**breaking**] return json objects from the write tools
- *(api)* log every request with method, path, status and latency
- *(api)* add a readiness endpoint that checks the database
- *(api)* bound concurrent requests and time out slow ones
- *(db)* set explicit pool sizes and a connection acquire timeout

### Fixed

- *(tools)* split summary sleep stages over the same 30 days as its nights ([#57](https://github.com/StephanMeijer/vitals-mcp/pull/57))
- *(tools)* take source as an enum, reject unknown values ([#56](https://github.com/StephanMeijer/vitals-mcp/pull/56))

### Other

- run clippy and fmt on the 9xx self-hosted runners
- *(tools)* type the retry and internal tool errors
- *(spec)* record that real Authelia tokens work for both clients ([#70](https://github.com/StephanMeijer/vitals-mcp/pull/70))
- *(tools)* cover reading_update and reading_delete argument errors ([#59](https://github.com/StephanMeijer/vitals-mcp/pull/59))
- *(main)* cover the public url to resource and host derivation ([#58](https://github.com/StephanMeijer/vitals-mcp/pull/58))

## [0.4.3](https://github.com/StephanMeijer/vitals-mcp/compare/v0.4.2...v0.4.3) - 2026-11-06

### Fixed

- *(api)* treat database restarts and overload as retryable ([#48](https://github.com/StephanMeijer/vitals-mcp/pull/48))

### Other

- pin timescaledb to 2.31.0-pg17 ([#51](https://github.com/StephanMeijer/vitals-mcp/pull/51))
- run the advisory audit weekly and update cargo deps with dependabot ([#50](https://github.com/StephanMeijer/vitals-mcp/pull/50))
- *(api)* every listed tool is callable over /api ([#49](https://github.com/StephanMeijer/vitals-mcp/pull/49))
- *(deps)* update rmcp to 3.9 and lockfile patches ([#54](https://github.com/StephanMeijer/vitals-mcp/pull/54))
- *(db)* drop the unused signed user settings from app transactions
- *(auth)* note that the verifier and middleware also guard /api

## [0.4.2](https://github.com/StephanMeijer/vitals-mcp/compare/v0.4.1...v0.4.2) - 2026-11-06

### Added

- *(api)* expose every mcp tool to the app over /api

## [0.4.1](https://github.com/StephanMeijer/vitals-mcp/compare/v0.4.0...v0.4.1) - 2026-11-06

### Other

- *(db)* drop row-level security and compress every hypertable

## [0.4.0](https://github.com/StephanMeijer/vitals-mcp/compare/v0.3.1...v0.4.0) - 2026-11-06

### Added

- *(db)* [**breaking**] move stored ecg and acc frames into per-sample rows
- *(db)* [**breaking**] store ecg and acc as one row per sample

## [0.3.1](https://github.com/StephanMeijer/vitals-mcp/compare/v0.3.0...v0.3.1) - 2026-11-06

### Added

- *(api)* accept delta-encoded ecg and acc arrays on ingest

## [0.3.0](https://github.com/StephanMeijer/vitals-mcp/compare/v0.2.0...v0.3.0) - 2026-11-05

### Added

- *(api)* [**breaking**] ingest Polar Loop skin temperature, sleep and Nightly Recharge

## [0.2.0](https://github.com/StephanMeijer/vitals-mcp/compare/v0.1.4...v0.2.0) - 2026-11-05

### Added

- *(api)* [**breaking**] register devices per user and tag measurements with their numeric id

### Fixed

- *(db)* release reader-pool session state after sql_query

## [0.1.4](https://github.com/StephanMeijer/vitals-mcp/compare/v0.1.3...v0.1.4) - 2026-11-05

### Fixed

- *(tools)* accept sql_query results with a column named t

## [0.1.3](https://github.com/StephanMeijer/vitals-mcp/compare/v0.1.2...v0.1.3) - 2026-11-05

### Fixed

- *(auth)* advertise openid and offline_access in resource metadata
- *(auth)* use the discovered issuer and require an origin public url
- *(tools)* cap reading_aggregate at 1000 buckets and time out app queries

## [0.1.2](https://github.com/StephanMeijer/vitals-mcp/compare/v0.1.1...v0.1.2) - 2026-11-04

### Fixed

- *(tools)* reject unknown keys in tool parameters
- *(auth)* finish a jwks refresh when its caller is cancelled
- *(auth)* accept the bearer scheme case-insensitively

## [0.1.1](https://github.com/StephanMeijer/vitals-mcp/compare/v0.1.0...v0.1.1) - 2026-11-04

### Added

- *(api)* add device ingest api with compressed raw strap storage

### Fixed

- *(auth)* bound jwks refresh and keep valid tokens off its lock
- *(tools)* reject unknown reading fields
- shut down gracefully on sigterm

### Other

- *(auth)* cover bearer middleware and metadata
- *(spec)* note sleep_stage chunk-creation deadlock
- cover sleep_update and sleep_delete
- *(dev)* bind dev database to localhost
