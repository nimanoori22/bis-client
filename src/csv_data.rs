use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// Unmodified SDMX period label, e.g. 2025-01-27, 2025-01 or 2025-Q1.
    pub period: String,
    /// Empty OBS_VALUE becomes None; zero and negative values remain numbers.
    pub value: Option<f64>,
    /// Every column, including dimensions, attributes and the original OBS_VALUE.
    /// These are not all dimensions: consult the dataflow's DSD for series identity.
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CsvData {
    pub columns: Vec<String>,
    pub observations: Vec<Observation>,
}

/// Parse a UTF-8, headered BIS observation CSV without discarding unknown columns.
/// Numeric codes and UNIT_MULT are preserved; values are not rescaled.
pub fn parse_csv(bytes: &[u8]) -> Result<CsvData> {
    let mut reader = csv::Reader::from_reader(bytes);
    let headers = reader.headers()?.clone();
    let mut unique = std::collections::BTreeSet::new();
    if headers
        .iter()
        .any(|h| h.trim().is_empty() || !unique.insert(h))
    {
        return Err(Error::InvalidCsv("empty or duplicate header".into()));
    }
    let period_index = headers
        .iter()
        .position(|h| h == "TIME_PERIOD")
        .ok_or_else(|| Error::InvalidCsv("missing TIME_PERIOD".into()))?;
    let value_index = headers
        .iter()
        .position(|h| h == "OBS_VALUE")
        .ok_or_else(|| Error::InvalidCsv("missing OBS_VALUE".into()))?;
    let mut observations = Vec::new();
    for (index, record) in reader.records().enumerate() {
        let record = record?;
        let period = record[period_index].to_owned();
        if period.trim().is_empty() {
            return Err(Error::InvalidCsv(format!(
                "empty period at record {}",
                index + 1
            )));
        }
        let text = record[value_index].trim();
        let value = if text.is_empty() {
            None
        } else {
            let value: f64 = text.parse().map_err(|_| {
                Error::InvalidCsv(format!(
                    "invalid OBS_VALUE at record {}: {text:?}",
                    index + 1
                ))
            })?;
            if !value.is_finite() {
                return Err(Error::InvalidCsv(format!(
                    "non-finite OBS_VALUE at record {}",
                    index + 1
                )));
            }
            Some(value)
        };
        observations.push(Observation {
            period,
            value,
            fields: headers
                .iter()
                .zip(record.iter())
                .map(|(h, v)| (h.to_owned(), v.to_owned()))
                .collect(),
        });
    }
    Ok(CsvData {
        columns: headers.iter().map(str::to_owned).collect(),
        observations,
    })
}

/// CSV rows for every detail mode, including series keys without TIME_PERIOD.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CsvTable {
    pub columns: Vec<String>,
    pub rows: Vec<BTreeMap<String, String>>,
}

pub fn parse_table(bytes: &[u8]) -> Result<CsvTable> {
    let mut reader = csv::Reader::from_reader(bytes);
    let headers = reader.headers()?.clone();
    let mut unique = std::collections::BTreeSet::new();
    if headers.is_empty()
        || headers
            .iter()
            .any(|h| h.trim().is_empty() || !unique.insert(h))
    {
        return Err(Error::InvalidCsv("empty or duplicate header".into()));
    }
    let rows = reader
        .records()
        .map(|record| {
            record.map(|record| {
                headers
                    .iter()
                    .zip(record.iter())
                    .map(|(h, v)| (h.to_owned(), v.to_owned()))
                    .collect()
            })
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(CsvTable {
        columns: headers.iter().map(str::to_owned).collect(),
        rows,
    })
}
