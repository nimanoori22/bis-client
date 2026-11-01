use crate::Error;
use chrono::{DateTime, Utc};
use reqwest::{StatusCode, header};
use rustopus::{
    Decision, HttpStatusError, Outcome, Policy, StatusPolicy, Transport, TransportError,
    TransportErrorKind,
};
use std::{
    error::Error as _,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

// Outcome has no final URL field. This private response header is overwritten
// after receiving headers and removed before returning the outcome to callers.
pub(crate) const FINAL_URL: &str = "x-bis-client-internal-final-url";

#[derive(Clone)]
pub(crate) struct BoundedTransport {
    pub http: reqwest::Client,
    pub limit: Arc<AtomicUsize>,
}

impl Transport for BoundedTransport {
    async fn send(
        &self,
        request: reqwest::Request,
    ) -> std::result::Result<Outcome, TransportError> {
        let limit = self.limit.load(Ordering::Relaxed);
        let mut response = self.http.execute(request).await?;
        let status = response.status();
        let mut headers = response.headers().clone();
        headers.insert(
            FINAL_URL,
            response
                .url()
                .as_str()
                .parse()
                .map_err(|e| TransportError::new(TransportErrorKind::Request, e))?,
        );
        let oversized = || {
            TransportError::new(
                TransportErrorKind::Request,
                Error::ResponseTooLarge { limit },
            )
        };
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err(oversized());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if chunk.len() > limit.saturating_sub(body.len()) {
                return Err(oversized());
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Outcome {
            status,
            headers,
            body: body.into(),
        })
    }
}

/// BIS status handling, preserving HTTP-date Retry-After and long-delay refusal.
/// rustopus supplies the shared limits, retry loop, backoff and deadline.
#[derive(Debug, Clone, Copy)]
pub struct BisPolicy;

impl Policy for BisPolicy {
    type Error = HttpStatusError;

    fn decide(
        &self,
        attempt: u32,
        result: &std::result::Result<Outcome, TransportError>,
    ) -> Decision<Self::Error> {
        if let Ok(outcome) = result {
            let fail = || {
                Decision::Fail(HttpStatusError {
                    status: outcome.status,
                    body: outcome.body.clone(),
                })
            };
            if outcome.status.is_success()
                && !matches!(outcome.status, StatusCode::OK | StatusCode::NO_CONTENT)
            {
                return fail();
            }
            if matches!(outcome.status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504)
                && outcome.headers.contains_key(header::RETRY_AFTER)
            {
                return match retry_delay(&outcome.headers, attempt.saturating_sub(1)) {
                    Some(delay) => Decision::Retry { after: Some(delay) },
                    None => fail(),
                };
            }
        }
        StatusPolicy::new().decide(attempt, result)
    }
}

pub(crate) fn retry_delay(headers: &header::HeaderMap, attempt: u32) -> Option<Duration> {
    let delay = headers
        .get(header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|value| {
            value
                .trim()
                .parse::<u64>()
                .ok()
                .map(Duration::from_secs)
                .or_else(|| {
                    DateTime::parse_from_rfc2822(value).ok().map(|date| {
                        (date.with_timezone(&Utc) - Utc::now())
                            .to_std()
                            .unwrap_or(Duration::ZERO)
                    })
                })
        })
        .unwrap_or(Duration::from_secs(1u64 << attempt.min(6)));
    (delay <= Duration::from_secs(60)).then_some(delay)
}

pub(crate) fn map_error(error: rustopus::Error<HttpStatusError>, url: &reqwest::Url) -> Error {
    match error {
        rustopus::Error::Policy(error) => Error::Http {
            status: error.status.as_u16(),
            url: url.to_string(),
            body: String::from_utf8_lossy(&error.body)
                .chars()
                .take(2048)
                .collect(),
        },
        rustopus::Error::Transport(error) => {
            if let Some(Error::ResponseTooLarge { limit }) =
                error.source().and_then(|e| e.downcast_ref::<Error>())
            {
                return Error::ResponseTooLarge { limit: *limit };
            }
            Error::Orchestration(rustopus::Error::Transport(error))
        }
        other => Error::Orchestration(other),
    }
}
