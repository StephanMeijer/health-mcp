# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/StephanMeijer/vitals-mcp/compare/v0.1.0...v0.1.1) - 2026-11-04

### Fixed

- *(auth)* bound jwks refresh and keep valid tokens off its lock
- *(tools)* reject unknown reading fields
- shut down gracefully on sigterm

### Other

- *(auth)* cover bearer middleware and metadata
- *(spec)* note sleep_stage chunk-creation deadlock
- cover sleep_update and sleep_delete
- *(dev)* bind dev database to localhost
