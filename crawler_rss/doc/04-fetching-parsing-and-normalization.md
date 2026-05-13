# Fetching, Parsing, And Normalization

## Overview

Feed extraction is implemented as a pipeline with four steps:

1. HTTP fetch
2. feed parsing
3. source normalization
4. final filtering

The output of the pipeline is always a `RawFeedScrapeResult`.

## Fetch Protection Strategy

Each feed carries a `fetchprotection` value. The fetcher uses it to decide which
request strategy to try.

Supported values in the current code:

- `0`: blocked immediately, no network request is attempted
- `1`: simple strategy
- `2`: advanced strategy

Behavior differs slightly depending on `ingest`:

- for ingest tasks, the fetcher first honors the requested protection mode and then falls back to the alternate mode
- for non-ingest tasks, it always tries simple then advanced

## HTTP Request Behavior

The fetcher builds a `reqwest` client with a request timeout and applies browser-
like default headers:

- `User-Agent`
- `Accept-Language`
- `Accept`
- `Accept-Encoding`

When advanced fetch protection is used and `host_header` is present, it also
injects:

- `Host`
- `Origin`
- `Referer`

Conditional request headers are supported:

- `If-None-Match` from `etag`
- `If-Modified-Since` from `last_update`

## Rate Limiting

Rate limiting is local and per effective host. The fetcher reserves a future
slot for each host and sleeps until that slot when necessary. This is separate
from the runtime's per-host concurrency cap:

- concurrency limits how many requests can run at once
- rate limiting controls how frequently a given host can be hit

## Retry Policy

Only transient failures are retried. The current transient categories are:

- timeout-like and connectivity-like `reqwest` errors
- HTTP `408`
- HTTP `429`
- HTTP `500`
- HTTP `502`
- HTTP `503`
- HTTP `504`

Retries use a small linear backoff: `250ms * attempt_no`.

Permanent failures return an error result immediately for the attempted strategy.

## Parsing

Parsing is deliberately small and delegated to `feed-rs`. The worker turns raw
bytes into a `feed_rs::model::Feed` and maps parser failures to
`RssWorkerError::InvalidPayload`.

This keeps the RSS/Atom format support inside a dedicated library instead of
re-implementing XML handling in the worker.

## Source Normalization

Normalization converts parsed feed entries into `RssSource` objects with:

- `title`
- `urls`
- `summary`
- `authors`
- `published_at`
- `image_url`

## Entry Selection Rules

An entry is dropped when any of the following is true:

- it has no usable links
- its primary link was already seen earlier in the same feed
- it has no publish/update timestamp and no `published_since` cutoff is available
- it is older than `last_db_article_published_at` when that cutoff is provided
- it has no usable normalized title

The deduplication done here is feed-local and primary-link-based.

## Summary Cleanup

Summary extraction prefers:

1. `entry.summary`
2. otherwise `entry.content.body`

The cleanup logic:

- strips HTML tags while preserving block-level spacing
- decodes HTML entities, with a second pass to catch nested escaping
- collapses whitespace
- trims boundary quotes

The goal is a display-ready plain-text summary, not rich HTML preservation.

## Author Extraction Heuristics

Author normalization is one of the most opinionated parts of the worker. The
implementation tries to turn messy bylines into stable person names by:

- splitting author lists and conjunctions
- removing editorial prefixes such as byline markers
- stripping parenthetical metadata
- dropping obvious role labels, domains, and location-only fragments
- normalizing accents and punctuation for duplicate detection

The worker keeps the cleaned display name but de-duplicates using a normalized
identity form.

## Image Extraction

Image selection tries several sources in order:

1. media thumbnails
2. media content tagged as image content
3. first inline `<img src=...>` found in summary or content
4. `content.src` when it looks like an image URL

This is a best-effort heuristic, not a guarantee of editorial hero-image quality.

## Final Filtering

After normalization, the worker drops sources with a `published_at` later than
the current clock value. This protects downstream ingestion from future-dated
items that would otherwise appear prematurely.
