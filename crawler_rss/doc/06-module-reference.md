# Module Reference

## Entry And Public Module Map

`src/main.rs`

- CLI entrypoint
- command dispatch for `run`, `set`, and `update`
- startup release validation
- creation of the gateway client, fetcher, and runtime loop

`src/lib.rs`

- module wiring
- stable internal names for the worker crate

## API, Auth, And Protocol

`src/api/client.rs`

- low-level HTTP JSON client
- bearer auth support
- response decoding and API error normalization

`src/api/gateway_client.rs`

- worker session lifecycle
- task claim requests
- completion and failure submission
- network retry loop and lease metadata tracking

`src/auth.rs`

- worker API key validation
- bearer token accessor

`src/protocol.rs`

- gateway request and response structs
- canonical JSON generation
- HMAC signing helpers
- nonce and timestamp generation

## Configuration And Paths

`src/config/config.rs`

- config file model
- default values
- environment and CLI override merge logic
- config load and save routines

`src/config/set.rs`

- mutation helper for `crawler_rss set`

`src/paths.rs`

- platform-specific Manifeed directory layout
- config, data, cache, state, and binary locations

`src/types.rs`

- worker type enumeration
- product naming used by release validation

## Runtime And Execution

`src/ports.rs`

- abstraction traits for the gateway, fetcher, and clock

`src/runtime/worker_runtime.rs`

- main runtime state machine
- join-set management for fetch and completion tasks
- state-report deduplication

`src/runtime/scheduling.rs`

- in-memory task and feed queue
- per-host scheduling logic
- task aggregation and progress labeling

`src/runtime/worker_state.rs`

- serializable worker state model
- equivalence rule for reporting noise reduction

`src/pipeline.rs`

- smallest execution unit for one scheduled feed
- conversion of fetcher errors into result payload errors

## Feed Processing

`src/fetch/fetch.rs`

- HTTP fetch execution
- headers and conditional request support
- rate limiting
- transient/permanent error classification
- fetch protection strategy selection

`src/parsing/parsing.rs`

- `feed-rs` integration

`src/filtering/filtering.rs`

- post-normalization filtering of future-dated entries

## Normalization

`src/normalize/normalize.rs`

- top-level entry normalization
- entry-level acceptance rules
- source construction

`src/normalize/text.rs`

- blank rejection
- whitespace collapsing
- quote trimming

`src/normalize/html.rs`

- HTML stripping and summary cleanup

`src/normalize/media.rs`

- image URL extraction heuristics

`src/normalize/authors.rs`

- author list splitting
- byline cleanup and duplicate detection

`src/normalize/author_patterns.rs`

- regular expressions backing author heuristics

`src/normalize/author_tokens.rs`

- lexical helpers used by author cleanup

`src/normalize/authors_tests.rs`

- focused tests for author extraction scenarios

## Task Payloads And Result Delivery

`src/model.rs`

- domain structs for feeds, tasks, results, and normalized sources

`src/claim/claim.rs`

- conversion from raw lease payload into `ClaimedRssTask`

`src/dedup/dedup.rs`

- completion payload assembly
- local dedup metadata block

`src/post_result/post_result.rs`

- concrete `RssGateway` implementation for the RSS worker
- claim parsing and completion payload submission

## Release And Maintenance

`src/app/release.rs`

- GitHub release manifest resolution
- version status classification
- cached manifest fallback

`src/app/update.rs`

- latest release lookup
- asset selection
- archive checksum verification
- self-update orchestration

`src/app/install.rs`

- tarball extraction
- binary swap on disk

## Logging And Errors

`src/logging.rs`

- optional stdout telemetry toggles

`src/error/error.rs`

- worker-wide error types
- auth detection
- user-facing error mapping
