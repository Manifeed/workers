# Runtime And Task Lifecycle

## Main Runtime Loop

The worker runtime lives in `runtime/worker_runtime.rs`. `RssWorker::run_once()`
processes a bounded unit of work:

- claim tasks if there is available capacity
- schedule feed fetches
- aggregate feed results into task-level completion payloads
- wait for backend acknowledgements of completed tasks
- return `true` when at least one task was processed, or `false` when nothing was claimed

The outer loop in `main.rs` decides when to sleep and how to react to fatal
authentication failures versus transient iteration failures.

## Claimed Task Shape

Each claimed task contains:

- `task_id`
- `execution_id`
- `job_id`
- `requested_at`
- `ingest`
- `feeds[]`

The `execution_id` matters because the same logical task may be retried or
re-leased by the backend. Runtime bookkeeping uses `(task_id, execution_id)` as
the effective key.

## Scheduling Model

`ClaimedFeedQueue` is the core in-memory scheduler. It keeps three views:

- `tasks`: task-level progress and result slots
- `pending_by_host`: queued feeds grouped by effective host
- `active_by_host`: currently running feeds grouped by effective host

This allows the runtime to enforce:

- a global cap on total concurrent feed fetches
- a per-host cap to avoid hammering the same upstream domain

## Effective Host Resolution

Host grouping does not rely only on the feed URL. The scheduler first tries:

1. `host_header`, when present
2. otherwise the hostname extracted from `feed_url`
3. otherwise a synthetic key based on `feed_id`

This matters because `host_header` can intentionally steer requests toward a
specific upstream host identity.

## Feed Execution Unit

Every scheduled feed becomes a micro-task executed through `pipeline.rs`. The
pipeline delegates the actual fetch to the `FeedFetcher` port and converts any
fetcher-level error into a `RawFeedScrapeResult::error`.

This means the runtime keeps moving even when individual feeds fail.

## Task Completion Semantics

Results are accumulated in the original feed order. A task is considered
complete only when every feed result slot has been filled.

When the last feed of a task finishes:

1. the scheduler produces a `CompletedTask`
2. the runtime spawns an async completion request
3. the task remains counted as pending until the gateway acknowledges the completion

This design prevents the worker from over-claiming tasks before already
finished work has been durably committed.

## State Tracking

The runtime can compute a `RssGatewayState` snapshot describing:

- whether the worker is active
- whether it is `idle`, `processing`, or `committing`
- how many tasks are pending
- the current task and current feed context
- the desired state

Equivalent states are de-duplicated before reporting. This avoids noisy updates
when only volatile feed-level details change.

Important implementation detail:

- the runtime computes these states
- the current `HttpRssGateway` implementation does not actually send them anywhere yet, because `update_state()` is currently a no-op

## Error Handling Strategy

The runtime distinguishes two broad classes:

- authentication errors: considered fatal, the process exits
- all other iteration errors: logged, then retried after a short sleep

Per-feed problems are usually encoded inside normal result payloads rather than
escalated as task-level runtime failures.

## What Is Not Used Yet

The gateway trait exposes `fail(task_id, execution_id, error_message)`, but the
current runtime path normally completes tasks with per-feed success or error
results instead of using `fail`. The `fail` path is available for future
unrecoverable task-level behavior.
