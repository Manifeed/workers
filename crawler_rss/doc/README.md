# Crawler RSS Docs Index

This folder contains detailed documentation for `workers/crawler_rss`.

## Documentation Map

- [`01-overview.md`](./01-overview.md): worker purpose, boundaries, and end-to-end flow
- [`02-cli-and-configuration.md`](./02-cli-and-configuration.md): CLI commands, config file behavior, and runtime tuning
- [`03-runtime-and-task-lifecycle.md`](./03-runtime-and-task-lifecycle.md): task claiming, scheduling, concurrency, and completion semantics
- [`04-fetching-parsing-and-normalization.md`](./04-fetching-parsing-and-normalization.md): HTTP fetch logic, feed parsing, filtering, and content normalization
- [`05-gateway-protocol-and-result-contract.md`](./05-gateway-protocol-and-result-contract.md): gateway protocol, request signing, task payloads, and result payloads
- [`06-module-reference.md`](./06-module-reference.md): module-by-module reference for everything under `src/`
- [`07-release-and-operations.md`](./07-release-and-operations.md): startup version checks, self-update flow, local paths, and operational notes

## Scope

These documents describe the current implementation in `src/`, including the
actual behavior of the runtime and the important constraints that affect
operations and future changes.

## Source of Truth

When the code changes, update this folder in the same pull request so the
documentation remains aligned with the implementation.
