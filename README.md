# bis-client

Standalone async Rust client for the public BIS SDMX REST v1 data and metadata
endpoints, using rustopus for shared request orchestration.
Returns typed observations and every original CSV field, without DataFrames or a
database dependency. Intended for ingestion adapters such as Winter's.

## Use

```toml
[dependencies]
bis-client = { git = "https://github.com/nimanoori22/bis-client.git" }
```

```rust,no_run
use bis_client::{Client, Query};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = Client::new()?;
let mut query = Query::new("WS_CBPOL", "D.JP");
query.start_period = Some("2025-01-23".into());
query.end_period = Some("2025-01-28".into());
let snapshot = client.fetch(&query).await?;

for row in &snapshot.data.observations {
    println!("{} {:?} {:?}", row.period, row.value, row.fields.get("UNIT_MEASURE"));
}

// Archive outside the library. Bytes are after HTTP decompression.
std::fs::write("japan-policy.csv", &snapshot.body)?;
println!("{} {}", snapshot.retrieved_at, snapshot.sha256);
# Ok(()) }
```

Convenience methods:

```rust,no_run
# async fn example() -> bis_client::Result<()> {
let client = bis_client::Client::new()?;
let full = client.fetch_full_history("WS_CBPOL", "D.JP").await?;
let recent = client.fetch_since("WS_CBPOL", "D.JP", "2025-01-01").await?;
# let _ = (full, recent);
# Ok(()) }
```

One `Client` should be reused; clones share a rustopus arm. `with_http_client`
accepts a configured reqwest client for proxies or other transport settings; enable gzip on that client if
wanted. Each attempt has a 60-second timeout. The default arm has two in-flight
slots,
three attempts, exponential backoff with jitter, and a 180-second total deadline
including queueing and retry waits. There is no default requests-per-second quota;
configure one for your application when needed. Transient transport failures and
HTTP 408/429/500/502/503/504 are retried. Retry-After seconds or HTTP dates are
respected; when the
requested delay exceeds 60 seconds, the HTTP error is returned instead of
retrying too early. Server-directed waits pause the shared arm, including client
clones. Exhausted attempts and total deadlines remain distinguishable under
`Error::Orchestration`. Response bodies are limited to 128 MiB after decompression;
`with_max_response_bytes` changes this for the client and its clones. Narrow
large queries when necessary.

## Data contract

- `Query` supports a flow ID (`WS_CBPOL`) or agency/flow/version
  (`BIS,WS_CBPOL,1.0`), plus SDMX dot-separated keys (`D.JP`, `D.JP+US`, `D.`).
- Query boundaries support `YYYY`, `YYYY-MM`, `YYYY-MM-DD`, `YYYY-Qn` and
  `YYYY-Sn`, `YYYY-Tn`, `YYYY-Mnn`, `YYYY-Wnn` (ISO week), `YYYY-Dnnn`
  (ordinal day) and `YYYY-A1`. Reporting periods may omit the hyphen after the
  year. Boundaries are inclusive and calendar-validated. Returned labels are
  preserved without assuming all series are daily.
- Optional first/last observation counts are limits, **not pagination**. No
  implicit last-N cap is used for full history.
- `detail=full` preserves attributes. Each observation retains **all** original
  columns in `fields`, including raw `OBS_VALUE`, unit, multiplier, status flags,
  source notes, and future/unknown columns. Header order is in `CsvData.columns`.
  Not every field is a series dimension: use the DSD when defining identity.
- `value` is a finite `f64` or `None` for an empty cell. Zero and negative values
  are retained. Values are not rescaled by `UNIT_MULT`; use the accompanying
  unit/multiplier. Exact original numeric text is in `fields["OBS_VALUE"]`.
- No sorting, deduplication, interpolation or policy-rate interpretation occurs.
- `FetchResult` includes the decoded CSV body, its SHA-256, collection time,
  requested/final URLs, status, content type, ETag and Last-Modified. ETag and
  Last-Modified are transport evidence, **not verified publication timestamps**.
- HTTP 204 returns an explicit empty result. HTTP 404 remains `Error::Http`;
  absence cannot be distinguished reliably from an invalid selection. HTML,
  partial HTTP responses, malformed records and non-finite values are errors.

## Discovery and CSV detail modes

```rust,no_run
use bis_client::{AvailabilityMode, Client, DataDetail, Query, StructureRef};

# async fn example() -> bis_client::Result<()> {
let client = Client::new()?;
let flows = client.dataflow(&StructureRef::new("BIS", "WS_EER", "latest")).await?;
if let Some(flow) = flows.first() {
    let id = StructureRef::from_urn(&flow.structure)?;
    let structures = client.datastructure(&id).await?;
    for structure in structures {
        // Sorted positional series dimensions; time is separate.
        for dimension in structure.dimensions() {
            println!("{} {}", dimension.position, dimension.id);
        }
    }
}
let codes = client.codelist(&StructureRef::new("BIS", "CL_FREQ", "latest")).await?;
let query = Query::new("WS_EER", "M.N.B.CH");
let availability = client.availability(&query, "all", AvailabilityMode::Exact).await?;
let keys = client.fetch_table(&query, DataDetail::SeriesKeysOnly).await?;
# let _ = (codes, availability, keys);
# Ok(()) }
```

`dataflows()` lists the latest BIS dataflows. `dataflow`, `datastructure`, and
`codelist` accept agency/id/version selectors, including `all`, `latest`, and
`+` selections where supported by BIS. Discovery requests use `references=none`;
resolve returned DSD and codelist URNs explicitly with `StructureRef::from_urn`.
Metadata is parsed as typed SDMX-JSON. Unmodelled fields, multilingual labels,
attribute relationships, measures and annotations remain in `extra` maps.
Malformed or missing collections are errors, rather than empty catalogues.

`fetch` continues returning `FetchResult<CsvData>` with full observations.
`fetch_table` returns `FetchResult<CsvTable>` with generic rows and supports all
four `DataDetail` variants: `Full`, `DataOnly`, `SeriesKeysOnly`, `NoData`.
Table rows retain string values and do not require observation columns.
`Query::url_with_detail` previews a URL without making a request.
Availability accepts period filters and `Exact` or `Available` mode; observation
count limits are rejected for that endpoint.

## Configure rustopus

```rust,no_run
use bis_client::{BisPolicy, Client, reqwest, rustopus::Arm};
use std::time::Duration;

# fn example() -> bis_client::Result<()> {
let http = reqwest::Client::builder().gzip(true).build()?;
let client = Client::with_arm_builder(
    http,
    Arm::builder()
        .policy(BisPolicy)
        .per_second(2) // Application policy, not a documented BIS quota.
        .suckers(2)
        .max_attempts(3)
        .total_timeout(Duration::from_secs(180)),
);
# let _ = client;
# Ok(()) }
```

Use `bis_client::reqwest` (re-exported from rustopus) for configured clients.
The direct reqwest dependency only enables gzip on that same version.
The rustopus dependency uses version 0.1.0 from crates.io.
`with_arm_builder` uses the supplied builder settings without adding defaults.
The BIS transport enforces the body-size limit during decompression/download,
retains the final redirect URL, and never exposes partially received bodies.
The BIS policy supports both numeric and HTTP-date `Retry-After`; waits over
60 seconds return an HTTP error instead of retrying earlier than instructed.

Version 0.2 uses reqwest 0.13 instead of 0.12. Update externally supplied clients
to the re-exported version. Retriable failures that exhaust attempts now return
`Error::Orchestration(rustopus::Error::Exhausted { .. })`; non-retriable HTTP
failures still return `Error::Http`. `FetchResult` now has a defaulted data type
parameter, so existing ordinary observation callers retain their result type.

## Winter integration boundary

The library does not depend on Winter's `MacroDataSource`. A future Winter
adapter can map its selected flow/key to `Query`, preserve source field metadata,
and turn observations into the domain batch type. Keep database writes,
checkpoint management, scheduling and availability policy in Winter.

**Do not pass Winter's collection timestamp directly to `fetch_since`.** This
method filters observation periods; it is not `updatedAfter`. The adapter can
re-fetch a suitable overlapping window and periodically refresh full history.
Corrections or deletions outside the overlap will otherwise be missed. A missing
row alone is not an authoritative deletion signal.

`fetch_full_history` returns today's available snapshot, not historical vintages.
An observation's period (including a policy effective date) is not necessarily its
announcement date or its availability to a trader. Archive retrieval evidence and
use Winter's snapshot/availability semantics; never invent historical release times.

The Japan series spans different policy instruments/regimes. A scalar may
represent a target-band midpoint; keep the compilation notes and do not infer
both target bounds from it. BIS release latency also remains relevant.

## Run

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check

# Explicit live network calls:
cargo run --example fetch -- WS_CBPOL D.JP 2025-01-23 2025-01-28
cargo run --example fetch -- WS_CBPOL M.JP 2025-01 2025-03
cargo run --example fetch -- WS_CBPOL D.JP
cargo run --example discover
```

Normal tests use localhost responses and a saved, unmodified official CSV.
Fixture URL, capture time and hash are in `tests/fixtures/manifest.json`.

## Sources and implementation scope

- [BIS v1 Swagger](https://stats.bis.org/api-doc/v1/#/Data%20queries/get_data__flow___key__all)
- [SDMX REST 1.4.0 data queries](https://github.com/sdmx-twg/sdmx-rest/blob/v1.4.0/v2_1/ws/rest/docs/4_4_data_queries.md)
- [HTTP representation negotiation](https://github.com/sdmx-twg/sdmx-rest/blob/v1.4.0/v2_1/ws/rest/docs/4_6_conneg.md)
- [BIS policy-rate definitions and release cadence](https://data.bis.org/topics/CBPOL)
- [sdmx1 source capabilities](https://github.com/khaeru/sdmx/blob/main/sdmx/sources.json)

This is an original, limited BIS client, not a port of sdmx1. No Python source or
tests were copied. General SDMX features are not automatically BIS capabilities.
Observation XML/JSON, automatic caching, `updatedAfter`,
`includeHistory`, automatic backfill scheduling and probability calculations are
outside this version. BIS data reuse remains subject to
[BIS statistical terms](https://data.bis.org/help/legal).
