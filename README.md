# Manifeed Workers

Rust workspace for the Manifeed RSS crawler. The `crawler_rss` worker is self-contained and
ships its own CLI, HTTP gateway client, local configuration, and execution pipeline.

## Workspace

- `crawler_rss/`: native RSS crawler, runnable from the CLI

## Runtime Flow

The RSS crawler follows the gateway flow:

1. open a `worker_session`
2. `claim` one or more `worker_tasks`
3. execute the task locally
4. send `complete` or `fail`
5. clean up local state only after backend acknowledgement

Key points:

- `crawler_rss` includes its own local gateway client for `sessions/open`, `tasks/claim`, `tasks/complete`, and `tasks/fail`
- each backend claim assigns an `execution_id` that is distinct from the `task_id`
- `complete` and `fail` are idempotent on the backend side for identical retries on an already finalized lease
- workers do not talk directly to PostgreSQL or Qdrant
- local status files remain optional telemetry for CLI diagnostics

## User Experience

- `crawler_rss` starts directly from the CLI with `crawler_rss run`
- `crawler_rss set --url ... --api-key ... --concurrency ...` initializes or updates the local configuration
- `crawler_rss update` installs the latest compatible GitHub release
- a standard installation only requires `url`, `api_key`, and `concurrency`
- persistent crawler configuration is stored in `crawler_rss.json`
- status files are written in a coalesced way to limit disk I/O on the hot path

## Useful Commands

```bash
cargo fmt --all
cargo clippy -p crawler_rss --release --all-targets
cargo test -p crawler_rss
cargo build --release -p crawler_rss
```

## Architecture Notes

- `dist/` is a locally generated artifact and is no longer versioned
- worker bundles are extracted into `~/.local/share/manifeed/<worker>/current`
- the `rss` family publishes `crawler_rss_bundle`
- bundles, packages, and the CLI verify their version through the GitHub `releases/latest` API of the `Manifeed/workers` repository
- GitHub bundle downloads are public; `crawler_rss run` requires a valid `rss_scrapper` worker API key on the gateway side

## GitHub Release Pipeline

The `.github/workflows/release.yml` workflow produces the public bundles consumed by `crawler_rss update`.

- trigger: push of a `v*` tag or `workflow_dispatch`
- matrix: linux x86_64, linux aarch64, linux armv7, macos x86_64, macos aarch64, windows x86_64
- artifact naming: `crawler_rss_bundle-<version>-<platform>-<arch>.tar.gz` plus the matching `.sha256`
- all non-native architectures use `cross` (armv7 only); Linux `aarch64` is built on a GitHub-hosted ARM runner
- each tarball contains a stripped `bin/crawler_rss[.exe]` and a `manifest.json`
- SHA-256 verification is mandatory on the client side: a release missing the `.sha256` file will make `crawler_rss update` fail

Raspberry Pi coverage:

| Model | Recommended OS | Rust target | Asset |
|---|---|---|---|
| Pi 4 / Pi 5 / Pi 3 (RPi OS 64-bit) | 64-bit | `aarch64-unknown-linux-gnu` | `linux-aarch64` |
| Pi 2 / Pi 3 / Pi Zero 2 (RPi OS 32-bit) | 32-bit (armv7) | `armv7-unknown-linux-gnueabihf` | `linux-arm` |
