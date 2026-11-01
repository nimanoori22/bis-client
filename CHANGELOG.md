# Changelog

## 0.2.0 — 2026-11-02

- Replaced the local retry loop with rustopus shared concurrency, rate-limit
  configuration, jittered backoff and total deadlines.
- Preserved streamed decoded-body limits, redirect provenance, gzip and
  HTTP-date Retry-After handling using a bounded transport and BIS policy.
- Added typed dataflow, DSD, codelist and availability discovery with unknown
  metadata retained, plus artefact URN resolution.
- Added all CSV detail modes and further calendar-validated SDMX periods.
- Breaking: configured clients now use reqwest 0.13; exhausted retries return
  Error::Orchestration; body-limit updates apply to all client clones.

## 0.1.0 — 2026-10-30

- Added async CSV retrieval for BIS SDMX REST v1 dataflows and series keys.
- Added inclusive period filters and current-snapshot full/since helpers.
- Preserved missing values, exact row fields, raw responses and retrieval evidence.
- Added bounded retry, response size limits, replay tests and a network example.

Limitations: no publication-vintage history, catalog/DSD discovery, database
integration or automatic scheduling. See README.md for Winter availability rules.
