use crate::orchestration::{BoundedTransport, FINAL_URL, map_error};
use crate::{BisPolicy, CsvData, CsvTable, DataDetail, Error, Query, Result};
use chrono::{DateTime, Utc};
use reqwest::{StatusCode, header};
use rustopus::{Arm, ArmBuilder, Outcome};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

/// A current snapshot and the evidence needed to archive/replay it.
#[derive(Debug, Clone)]
pub struct FetchResult<T = CsvData> {
    pub request_url: String,
    pub response_url: String,
    pub retrieved_at: DateTime<Utc>,
    pub status: u16,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// SHA-256 of `body` (after HTTP decompression, before CSV parsing).
    pub sha256: String,
    pub body: Vec<u8>,
    pub data: T,
}

/// Reusable client. Clones share the HTTP pool, rustopus arm and body limit.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    arm: Arm<BisPolicy, BoundedTransport>,
    base_url: String,
    max_response_bytes: Arc<AtomicUsize>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("base_url", &self.base_url)
            .field(
                "max_response_bytes",
                &self.max_response_bytes.load(Ordering::Relaxed),
            )
            .finish_non_exhaustive()
    }
}

impl Client {
    pub fn new() -> Result<Self> {
        Ok(Self::with_http_client(
            reqwest::Client::builder()
                .user_agent(concat!("bis-client/", env!("CARGO_PKG_VERSION")))
                .redirect(reqwest::redirect::Policy::limited(5))
                .gzip(true)
                .build()?,
        ))
    }

    /// Supply rustopus's reqwest version, for example to configure a proxy.
    pub fn with_http_client(http: reqwest::Client) -> Self {
        Self::with_arm_builder(
            http,
            Arm::builder()
                .policy(BisPolicy)
                .suckers(2)
                .max_attempts(3)
                .total_timeout(Duration::from_secs(180)),
        )
    }

    /// Configure shared rate limits, concurrency, retries and total deadline.
    /// The builder must use BisPolicy to retain BIS error and Retry-After handling.
    pub fn with_arm_builder(http: reqwest::Client, builder: ArmBuilder<BisPolicy>) -> Self {
        let max_response_bytes = Arc::new(AtomicUsize::new(128 * 1024 * 1024));
        let arm = builder.build_with(BoundedTransport {
            http: http.clone(),
            limit: max_response_bytes.clone(),
        });
        Self {
            http,
            arm,
            base_url: crate::query::BASE_URL.into(),
            max_response_bytes,
        }
    }

    #[cfg(test)]
    fn make_arm(&self, builder: ArmBuilder<BisPolicy>) -> Arm<BisPolicy, BoundedTransport> {
        builder.build_with(BoundedTransport {
            http: self.http.clone(),
            limit: self.max_response_bytes.clone(),
        })
    }

    /// Changes the decoded body limit for this client and all its clones.
    pub fn with_max_response_bytes(self, bytes: usize) -> Self {
        self.max_response_bytes.store(bytes, Ordering::Relaxed);
        self
    }

    pub(crate) async fn request(
        &self,
        url: reqwest::Url,
        accept: &str,
    ) -> Result<(Outcome, String)> {
        let request = self
            .http
            .get(url.clone())
            .header(header::ACCEPT, accept)
            .timeout(Duration::from_secs(60))
            .build()?;
        let mut outcome = self
            .arm
            .send(request)
            .await
            .map_err(|e| map_error(e, &url))?;
        let final_url = outcome
            .headers
            .remove(FINAL_URL)
            .and_then(|v| v.to_str().ok().map(str::to_owned))
            .ok_or_else(|| Error::InvalidResponse("missing final response URL".into()))?;
        Ok((outcome, final_url))
    }

    /// Download observations with full attributes, retaining retrieval evidence.
    pub async fn fetch(&self, query: &Query) -> Result<FetchResult> {
        self.fetch_csv(query, DataDetail::Full, crate::parse_csv)
            .await
    }

    /// Download any CSV detail mode, including keys-only and no-observation rows.
    pub async fn fetch_table(
        &self,
        query: &Query,
        detail: DataDetail,
    ) -> Result<FetchResult<CsvTable>> {
        self.fetch_csv(query, detail, crate::parse_table).await
    }

    async fn fetch_csv<T: Default>(
        &self,
        query: &Query,
        detail: DataDetail,
        parse: fn(&[u8]) -> Result<T>,
    ) -> Result<FetchResult<T>> {
        let url = query.url_on_with_detail(&self.base_url, detail)?;
        let (outcome, response_url) = self
            .request(url.clone(), "application/vnd.sdmx.data+csv;version=1.0.0")
            .await?;
        let retrieved_at = Utc::now();
        let get_header = |name| {
            outcome
                .headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let content_type = get_header(header::CONTENT_TYPE);
        let data = if outcome.status == StatusCode::NO_CONTENT {
            T::default()
        } else {
            ensure_content_type(
                content_type.as_deref(),
                &[
                    "text/csv",
                    "application/csv",
                    "application/vnd.sdmx.data+csv",
                ],
            )?;
            parse(&outcome.body)?
        };
        Ok(FetchResult {
            request_url: url.to_string(),
            response_url,
            retrieved_at,
            status: outcome.status.as_u16(),
            content_type,
            etag: get_header(header::ETAG),
            last_modified: get_header(header::LAST_MODIFIED),
            sha256: format!("{:x}", Sha256::digest(&outcome.body)),
            body: outcome.body.to_vec(),
            data,
        })
    }

    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    /// All currently available observations; this does not retrieve old vintages.
    pub async fn fetch_full_history(&self, flow: &str, key: &str) -> Result<FetchResult> {
        self.fetch(&Query::new(flow, key)).await
    }

    /// Inclusive observation-period filter, NOT changes since a publication time.
    /// Old revisions/deletions require wider re-fetches or periodic full snapshots.
    pub async fn fetch_since(
        &self,
        flow: &str,
        key: &str,
        start_period: &str,
    ) -> Result<FetchResult> {
        let mut query = Query::new(flow, key);
        query.start_period = Some(start_period.to_owned());
        self.fetch(&query).await
    }
}

pub(crate) fn ensure_content_type(value: Option<&str>, allowed: &[&str]) -> Result<()> {
    let mime = value
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !allowed.contains(&mime.as_str()) {
        return Err(Error::ContentType(value.unwrap_or("").into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
