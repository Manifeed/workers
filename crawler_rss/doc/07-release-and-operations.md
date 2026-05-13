# Release And Operations

## Startup Release Validation

Before entering the runtime loop, `crawler_rss run` checks whether the current
binary version is still acceptable relative to the latest GitHub release.

The check classifies the installed version as:

- `up_to_date`
- `update_available`
- `incompatible`
- `unverified`

Behavior:

- `incompatible`: startup fails
- `update_available`: startup continues and logs a warning
- `unverified`: startup continues and logs a warning
- `up_to_date`: startup continues silently

## Manifest Source And Cache

Release metadata is fetched from the GitHub repository configured by:

- `MANIFEED_WORKER_GITHUB_REPOSITORY`
- default: `Manifeed/workers`

If the network lookup fails, the worker tries to use a cached manifest stored
under the version cache directory.

## Self-Update Flow

`crawler_rss update` performs these steps:

1. query GitHub `releases/latest`
2. select the current platform/architecture bundle
3. download the `.tar.gz`
4. download the matching `.sha256`
5. verify the archive hash
6. extract the archive
7. locate the `crawler_rss` binary
8. replace the current executable with a backup-and-swap sequence

On Unix, the installed replacement is marked executable with `0755`.

## Platform-Specific Storage

The worker resolves five base directories:

- `config_dir`
- `data_dir`
- `cache_dir`
- `state_dir`
- `bin_dir`

In the current code, the most important actively used paths are:

- config file path
- update cache directory
- version manifest cache directory

## Operational Characteristics

- the process is intended to run continuously
- idle loops sleep using `poll_seconds`
- transient loop failures sleep for `3` seconds before retrying
- authentication failures are treated as fatal

## Reliability Notes

- completion acknowledgements are retried before task lease metadata is forgotten
- feed-level failures remain visible in result payloads instead of disappearing as runtime noise
- config writes are atomic at the file level through temp-file replacement
- update installation refuses unsigned-equivalent releases missing checksum assets

## Current Limitations

- runtime state computation exists, but gateway state reporting is not yet wired in the HTTP gateway implementation
- local dedup metadata is present in the result contract, but cross-result duplicate removal is not implemented yet
- self-update currently relies on the system `tar` command being available
- only the RSS worker family is modeled in `WorkerType`
