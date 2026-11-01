#![doc = include_str!("../README.md")]
pub mod csv_data;
pub use csv_data::{CsvData, CsvTable, Observation, parse_csv, parse_table};
mod query;
pub use query::{DataDetail, Query};
mod client;
pub use client::{Client, FetchResult};
mod orchestration;
pub use orchestration::BisPolicy;
mod metadata;
pub use metadata::*;
pub use rustopus;
/// Use this reqwest version when supplying an HTTP client.
pub use rustopus::reqwest;

/// Retrieval and validation failures. HTTP 404 is deliberately not an empty dataset.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("invalid CSV: {0}")]
    InvalidCsv(String),
    #[error(transparent)]
    Csv(#[from] csv::Error),
    #[error(transparent)]
    Transport(#[from] reqwest::Error),
    #[error("BIS HTTP {status} for {url}: {body}")]
    Http {
        status: u16,
        url: String,
        body: String,
    },
    #[error(transparent)]
    Orchestration(#[from] rustopus::Error<rustopus::HttpStatusError>),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("invalid BIS response: {0}")]
    InvalidResponse(String),
    #[error("unexpected Content-Type: {0}")]
    ContentType(String),
    #[error("response exceeds {limit} decoded bytes")]
    ResponseTooLarge { limit: usize },
}

pub type Result<T> = std::result::Result<T, Error>;
