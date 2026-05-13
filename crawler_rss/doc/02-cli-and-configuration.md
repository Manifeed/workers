# CLI And Configuration

## CLI Surface

The binary exposes three top-level commands:

- `crawler_rss run`
- `crawler_rss set`
- `crawler_rss update`

If no command is provided, the binary defaults to `run`.

## `run`

`run` is the operational mode. It:

1. optionally enables stdout telemetry with `--log`
2. loads the merged runtime configuration
3. validates release compatibility
4. instantiates the gateway client and fetcher
5. enters the endless `run_once()` loop

Supported arguments:

- `--config <path>`: custom config file path
- `--url <api-url>`: override backend URL
- `--api-key <worker-api-key>`: override worker credential
- `--concurrency <n>`: override max global in-flight feed requests
- `--log`: enable extra stdout log lines from the hot path

## `set`

`set` updates or initializes the local JSON configuration file. It persists:

- `api_url`
- `api_key`
- `concurrency`

The command can also print the resulting config, with API key redaction unless
explicit secret display is requested by the caller.

## `update`

`update` queries the latest GitHub release, selects the platform-compatible
bundle for the current machine, verifies the SHA-256 checksum, extracts the
archive, and replaces the currently running executable on disk.

`--dry-run` resolves the candidate release without installing it.

## Configuration Sources And Precedence

Runtime configuration merges multiple sources. For the core values, the worker
follows this order:

1. command-line override
2. worker-specific environment variable
3. broader worker environment variable
4. stored JSON config value
5. built-in default

Key examples:

- API URL: CLI `--url`, then `MANIFEED_CRAWLER_RSS_API_URL`, then `MANIFEED_WORKER_API_URL`, then `MANIFEED_API_URL`, then stored/default value
- API key: CLI `--api-key`, then `MANIFEED_CRAWLER_RSS_API_KEY`, then `MANIFEED_WORKER_API_KEY`, then stored value
- global concurrency: CLI `--concurrency`, then `MANIFEED_CRAWLER_RSS_CONCURRENCY`, then stored value

## Built-In Defaults

The current implementation uses these built-in defaults:

| Setting | Default |
|---|---|
| API URL | `https://api.manifeed.app` |
| Poll interval | `5` seconds |
| Lease duration | `120` seconds |
| Session TTL | `3600` seconds |
| Host max requests per second | `10` |
| Max in-flight requests | `5` |
| Max in-flight requests per host | `3` |
| Request timeout | `10` seconds |
| Fetch retry count | `1` |

## Config File

The persistent config file is `crawler_rss.json`. By default it is stored under
the platform-specific Manifeed config directory:

- Linux: `${XDG_CONFIG_HOME:-~/.config}/manifeed/crawler_rss.json`
- macOS: `~/Library/Application Support/Manifeed/crawler_rss.json` through `directories`
- Windows: `%APPDATA%/Manifeed/crawler_rss.json` through `directories`

The file contains:

- `schema_version`
- `api_url`
- `api_key`
- `concurrency`

The save path uses a temporary file followed by rename, and on Unix the file is
restricted to `0600`.

## Validation Behavior

- blank API URL is rejected when building the API client
- blank API key is rejected by the authenticator
- concurrency-like numeric values are normalized with a minimum of `1`
- invalid environment variable parsing fails fast with a configuration error
