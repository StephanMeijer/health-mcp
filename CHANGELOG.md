# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
