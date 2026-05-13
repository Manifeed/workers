# Crawler RSS Overview

## Purpose

`crawler_rss` is the Manifeed worker responsible for fetching RSS and Atom
feeds, extracting article candidates, normalizing the content into a stable
shape, and sending the task result back to the worker gateway.

The worker is designed as a standalone Rust CLI. It does not depend on direct
database access and does not write final business data itself. Its main
responsibility is execution of fetch jobs delegated by the backend.

## What the Worker Owns

- command-line entrypoint and local configuration bootstrap
- worker session and task claim lifecycle
- HTTP feed download with conditional headers and retry handling
- feed parsing and source normalization
- local concurrency scheduling, including per-host throttling
- result packaging and signed completion requests
- startup version compatibility checks and self-update support

## What the Worker Does Not Own

- task creation and task orchestration logic in backend services
- persistent storage of articles, feeds, leases, or sessions
- direct access to PostgreSQL, Qdrant, or other storage backends
- final ingestion semantics beyond the normalized payload returned to the gateway

## High-Level Flow

1. The CLI loads configuration from file, environment variables, and command-line overrides.
2. Startup verifies whether the installed binary is still supported against the latest GitHub release metadata.
3. The worker opens or refreshes a gateway session.
4. It claims one or more RSS tasks, each containing a list of feeds.
5. Feeds are scheduled with both global concurrency and per-host concurrency limits.
6. Each feed is fetched, parsed, normalized, and converted to a `RawFeedScrapeResult`.
7. Once all feeds for a task finish, the worker sends a single `complete` request for that task.
8. The loop repeats; when nothing is claimed, the worker sleeps for the configured poll interval.

## Architectural Shape

The implementation is intentionally split into a few narrow layers:

- CLI and bootstrapping in `main.rs`
- pure configuration and path resolution in `config/` and `paths.rs`
- gateway protocol and HTTP transport in `api/`, `auth.rs`, and `protocol.rs`
- business runtime orchestration in `runtime/`, `pipeline.rs`, and `ports.rs`
- feed-specific behavior in `fetch/`, `parsing/`, `normalize/`, and `filtering/`
- result contract assembly in `dedup/` and `post_result/`

## Current Runtime Character

The worker favors resilience and bounded parallelism over aggressive throughput:

- network operations are retried only for transient failures
- each feed always returns a result object, even when the fetch fails
- task completion is delayed until every feed in the claimed task has finished
- gateway completion acknowledgements are retried before local lease metadata is dropped
- state-reporting structures exist in the runtime, but the current HTTP gateway implementation does not forward them yet
