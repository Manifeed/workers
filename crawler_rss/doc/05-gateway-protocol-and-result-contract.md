# Gateway Protocol And Result Contract

## HTTP Client Layer

`api/client.rs` provides a small JSON-oriented HTTP client with:

- base URL normalization
- optional bearer token injection
- JSON serialization and deserialization
- consistent API error mapping

For local HTTPS development against `localhost`, the client can accept invalid
certificates. This exception is intentionally restricted to localhost-like hosts.

## Authentication Model

The worker currently uses a bearer token equal to the configured worker API key.
There is no extra token exchange step on the worker side.

## Session And Claim Protocol

The gateway client performs two core operations before work execution:

1. open or refresh a worker session through `/workers/api/sessions/open`
2. claim tasks through `/workers/api/tasks/claim`

The session is refreshed when it is missing or close to expiry.

The claim request includes:

- `session_id`
- `task_type`
- `worker_version`
- `count`
- `lease_seconds`

## Lease Metadata Retention

For each claimed task, the client stores gateway metadata in memory:

- `session_id`
- `lease_id`
- `trace_id`
- `task_type`
- `worker_version`

This metadata is later required to build `complete` or `fail` requests for the
exact lease that was claimed.

## Request Signing

Completion and failure payloads are signed with HMAC-SHA256.

The implementation:

1. derives a secret by SHA-256 hashing the API key
2. builds a canonical JSON payload with stable key ordering
3. signs that canonical payload
4. sends the final request body including `signed_at`, `nonce`, and `signature`

This makes payload integrity independent from JSON field ordering in standard
serializer output.

## Network Retry Behavior

Gateway claim and result submission operations retry only on retryable network
errors. The current gateway client uses:

- up to `5` attempts
- `5` seconds between attempts

Lease metadata is removed only after a successful `complete` or `fail` response.

## Claimed Task Payload Contract

The RSS task payload decoded from a lease contains:

- `job_id`
- `requested_at`
- `ingest`
- `feeds`

Each feed carries:

- feed identity and URL
- optional company context
- optional host override
- fetch protection mode
- cache validators (`etag`, `last_update`)
- downstream cutoff marker (`last_db_article_published_at`)

## Completion Payload Contract

Task completion uses `WorkerRssTaskResultPayload`, which contains:

- `contract_version`
- `result_events`
- `local_dedup`

`result_events` is the vector of feed-level `RawFeedScrapeResult` objects.

Each result includes:

- task correlation fields: `job_id`, `ingest`
- feed identity: `feed_id`, `feed_url`
- status: `success`, `not_modified`, or `error`
- HTTP metadata: `status_code`, `new_etag`, `new_last_update`
- fetch protection traceability: requested and resolved protection values
- normalized `sources`

## Local Dedup Block

The current local dedup block is intentionally conservative:

- it reports the contract version and counting metadata
- it does not yet remove duplicates across task results
- `duplicates_dropped` is currently `0`
- `groups` is currently empty

This means the payload contract is already shaped for richer future dedup logic,
but the current implementation mostly forwards raw normalized candidates.
