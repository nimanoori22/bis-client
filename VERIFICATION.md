# Verification

## 2026-11-02 — rustopus and discovery extension

- `cargo test --locked`: 31 offline tests and 4 compile-only doctests passed.
- `cargo clippy --all-targets --locked -- -D warnings`: passed.
- `cargo fmt --check`: passed.
- `cargo run --example discover`: live latest-BIS catalogue retrieval, WS_EER
  DSD and frequency-codelist resolution, and availability lookup passed. The
  bounded M.N.B.CH selection for 2024-01 through 2024-02 returned 2 observations;
  DataOnly returned 2 rows, SeriesKeysOnly 1 row, and NoData 1 row.
- `cargo run --example fetch -- WS_CBPOL D.JP`: current full snapshot passed
  through the bounded rustopus transport: 24,914 observations, 62,866,664 decoded
  bytes, periods 1946-01-01 through 2026-10-27. SHA-256:
  `d7848f89dec4125d72783cdf1a3f186154b0b8df0916c9ab57a3813a6d1b0dc1`.
  Retrieved at 2026-11-01T21:17:56.035497010Z (2026-11-02 Asia/Tehran).
- Regression tests cover redirects and final-URL header spoofing, clone-wide
  Retry-After pauses, total deadlines, announced and chunked body limits,
  incomplete-response retries, explicit 204 emptiness and non-retried 404s.
- Confirmed one reqwest version (0.13.5) shared with the sibling rustopus checkout.
- Existing source was entirely untracked before this change. It was not staged,
  committed, or replaced; a baseline was saved outside the repository for review.

Live checks are current snapshots, not historical publication-vintage evidence.
BIS v2, XML/data-JSON parsing, caching and ingestion scheduling remain out of scope.

## Previous verification

2026-10-30 (Asia/Tehran). Current snapshot checks, not point-in-time backtests.

- japan-full: 24914 observations; 1946-01-01 through 2026-10-27.
- japan-monthly: 3 observations; 2025-01 through 2025-03.
- japan-reer: 3 observations; 2025-01 through 2025-03.
- policy-multicountry: 10 observations; 2025-01-23 through 2025-01-28.

21 offline tests and 2 compile-only doctests passed. Strict Clippy over all targets and formatting passed.

The full Japan request downloaded and parsed 62,866,664 decoded bytes. Raw full-history CSV is returned to callers but was not persisted by the example; only its hash and summary were saved. Daily and monthly test CSVs are preserved separately.

The cross-country daily selection returned 4 Japan observations and 6 US observations in the checked range; a common calendar must not be assumed.

No Winter adapter or live database writes were performed. No sdmx1 runtime comparison or BIS catalog discovery was performed.

```json
[
  {
    "case": "japan-full",
    "request_url": "https://stats.bis.org/api/v1/data/WS_CBPOL/D.JP/all?detail=full",
    "retrieved_at": "2026-10-29T23:04:42.295078628Z",
    "status": 200,
    "observations": 24914,
    "decoded_bytes": 62866664,
    "sha256": "d7848f89dec4125d72783cdf1a3f186154b0b8df0916c9ab57a3813a6d1b0dc1",
    "first_period": "1946-01-01",
    "last_period": "2026-10-27"
  },
  {
    "case": "japan-monthly",
    "request_url": "https://stats.bis.org/api/v1/data/WS_CBPOL/M.JP/all?detail=full&startPeriod=2025-01&endPeriod=2025-03",
    "retrieved_at": "2026-10-29T23:06:23.178608608Z",
    "status": 200,
    "observations": 3,
    "decoded_bytes": 7735,
    "sha256": "0d175575121509db87d2522fab4a9c64f736eb9390be8affa3be3fba0590303e",
    "first_period": "2025-01",
    "last_period": "2025-03"
  },
  {
    "case": "japan-reer",
    "request_url": "https://stats.bis.org/api/v1/data/WS_EER/M.R.B.JP/all?detail=full&startPeriod=2025-01&endPeriod=2025-03",
    "retrieved_at": "2026-10-29T23:07:05.210427371Z",
    "status": 200,
    "observations": 3,
    "decoded_bytes": 405,
    "sha256": "10cac19e872e0a13ef807ab888bf7c67c6d0900e5313c5cb66cba1fda77f28b1",
    "first_period": "2025-01",
    "last_period": "2025-03"
  },
  {
    "case": "policy-multicountry",
    "request_url": "https://stats.bis.org/api/v1/data/WS_CBPOL/D.JP+US/all?detail=full&startPeriod=2025-01-23&endPeriod=2025-01-28",
    "retrieved_at": "2026-10-29T23:07:04.743057065Z",
    "status": 200,
    "observations": 10,
    "decoded_bytes": 11949,
    "sha256": "bfcfe2f1d390aa9eab8822639e9e4b3e693fae8a3f4c0db788c85465c070de69",
    "first_period": "2025-01-23",
    "last_period": "2025-01-28"
  }
]
```
