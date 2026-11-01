# BIS client scope

- Standalone async Rust library; no Winter, database, or DataFrame dependencies.
- BIS SDMX REST v1 data endpoint, explicitly negotiated SDMX-CSV 1.0.
- Typed JSON dataflow/DSD/codelist and availability discovery; preserve unknown
  metadata in extra maps. URN resolution and sorted series dimensions.
- All CSV detail modes through fetch_table; ordinary fetch retains full observations.
- Any dataflow/key with validated URL components; inclusive period filters and
  first/last observation limits. Initial live cases: daily/monthly policy rates.
- Full history means the current available snapshot, not publication vintages.
  Since means an inclusive observation-period filter, not a changes feed.
- Preserve every CSV field, original numeric text, missing values, period labels,
  status flags, units and source notes. Return parsed rows plus original decoded
  CSV bytes, their SHA-256, request URL and retrieval time.
- Fail on malformed CSV, duplicate/missing headers, non-finite/invalid values,
  invalid requests, unexpected content types, and oversized responses.
- HTTP 204 is an explicit empty response. HTTP 404 stays a distinct error, since
  it cannot reliably distinguish an invalid series from an empty selection.
- rustopus arm shared across clones: two concurrent requests, three attempts,
  jittered backoff, 180-second total deadline by default; configurable rate quota
  and arm settings. Transient network errors and HTTP 408/429/500/502/503/504;
  respect Retry-After (seconds or HTTP date). No automatic retry of parser errors.
- Default response limit 128 MiB and per-request timeout 60 seconds.
- Fixture tests and localhost HTTP tests are offline. External verification is an
  explicit example, not part of cargo test.
- No observation XML/JSON parser, server caching, scheduler, database
  writes, true updatedAfter/includeHistory, or probability model in this version.

Acceptance: offline tests, doctest, strict Clippy, formatting, plus bounded live
requests and a complete Japan policy-rate snapshot when the service is reachable.

0.2 migration: use rustopus reqwest 0.13; exhausted retries retain the rustopus
Exhausted error; response limit changes are shared across clones.
