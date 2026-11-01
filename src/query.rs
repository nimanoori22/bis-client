use crate::{Error, Result};
use chrono::{NaiveDate, Weekday};

pub(crate) const BASE_URL: &str = "https://stats.bis.org/api/v1/";

/// The amount of data returned by the BIS CSV endpoint.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DataDetail {
    #[default]
    Full,
    DataOnly,
    SeriesKeysOnly,
    NoData,
}

impl DataDetail {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::DataOnly => "dataonly",
            Self::SeriesKeysOnly => "serieskeysonly",
            Self::NoData => "nodata",
        }
    }
}

/// BIS data selection. Both period boundaries are inclusive.
#[derive(Debug, Clone)]
pub struct Query {
    pub flow: String,
    pub key: String,
    pub start_period: Option<String>,
    pub end_period: Option<String>,
    pub first_n_observations: Option<usize>,
    pub last_n_observations: Option<usize>,
}

impl Query {
    pub fn new(flow: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            flow: flow.into(),
            key: key.into(),
            start_period: None,
            end_period: None,
            first_n_observations: None,
            last_n_observations: None,
        }
    }

    /// Build and validate a URL without making a request.
    pub fn url(&self) -> Result<reqwest::Url> {
        self.url_on(BASE_URL)
    }

    /// Build a URL for a CSV detail mode, including modes without observations.
    pub fn url_with_detail(&self, detail: DataDetail) -> Result<reqwest::Url> {
        self.url_on_with_detail(BASE_URL, detail)
    }

    pub(crate) fn url_on(&self, base: &str) -> Result<reqwest::Url> {
        self.url_on_with_detail(base, DataDetail::Full)
    }

    pub(crate) fn url_on_with_detail(
        &self,
        base: &str,
        detail: DataDetail,
    ) -> Result<reqwest::Url> {
        let safe = |s: &str, extra: &str| {
            !s.is_empty()
                && s.len() <= 2048
                && s != "."
                && s != ".."
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || extra.as_bytes().contains(&c))
        };
        if self.flow.split(',').count() > 3
            || !self.flow.split(',').all(|s| safe(s, "_.-@$"))
            || !safe(&self.key, "_.-+@$")
            || !self.key.split('.').all(|dimension| {
                dimension.is_empty() || dimension.split('+').all(|code| safe(code, "_-@$"))
            })
        {
            return Err(Error::InvalidQuery("invalid dataflow or key".into()));
        }
        let start = self
            .start_period
            .as_deref()
            .map(period_bounds)
            .transpose()?;
        let end = self.end_period.as_deref().map(period_bounds).transpose()?;
        if let (Some((start, _)), Some((_, end))) = (start, end)
            && start > end
        {
            return Err(Error::InvalidQuery("startPeriod is after endPeriod".into()));
        }
        if self.first_n_observations == Some(0) || self.last_n_observations == Some(0) {
            return Err(Error::InvalidQuery(
                "observation limits must be positive".into(),
            ));
        }
        let mut url = reqwest::Url::parse(base).map_err(|e| Error::InvalidQuery(e.to_string()))?;
        url.path_segments_mut()
            .map_err(|_| Error::InvalidQuery("invalid base URL".into()))?
            .pop_if_empty()
            .extend(["data", &self.flow, &self.key, "all"]);
        {
            let mut params = url.query_pairs_mut();
            params.append_pair("detail", detail.as_str());
            if let Some(value) = &self.start_period {
                params.append_pair("startPeriod", value);
            }
            if let Some(value) = &self.end_period {
                params.append_pair("endPeriod", value);
            }
            if let Some(value) = self.first_n_observations {
                params.append_pair("firstNObservations", &value.to_string());
            }
            if let Some(value) = self.last_n_observations {
                params.append_pair("lastNObservations", &value.to_string());
            }
        }
        Ok(url)
    }
}

// Only query boundaries are interpreted. Returned SDMX period labels stay untouched.
fn period_bounds(value: &str) -> Result<(NaiveDate, NaiveDate)> {
    let invalid = || {
        Error::InvalidQuery(format!(
            "unsupported or invalid period: {value:?}; use an ISO date or SDMX annual, semester, trimester, quarter, month, week or ordinal-day period"
        ))
    };
    if !value.is_ascii() || value.len() < 4 || !value.as_bytes()[..4].iter().all(u8::is_ascii_digit)
    {
        return Err(invalid());
    }
    let year: i32 = value[..4].parse().map_err(|_| invalid())?;
    if !(1..=9998).contains(&year) {
        return Err(invalid());
    }
    if value.len() == 10 {
        let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| invalid())?;
        if date.format("%Y-%m-%d").to_string() != value {
            return Err(invalid());
        }
        return Ok((date, date));
    }
    let suffix = value[4..].strip_prefix('-').unwrap_or(&value[4..]);
    if suffix.is_empty() && value.len() != 4 {
        return Err(invalid());
    }
    let number = |s: &str| -> Result<u32> {
        if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
            return Err(invalid());
        }
        s.parse().map_err(|_| invalid())
    };
    if let Some(week) = suffix.strip_prefix('W') {
        if week.len() != 2 {
            return Err(invalid());
        }
        let week = number(week)?;
        return Ok((
            NaiveDate::from_isoywd_opt(year, week, Weekday::Mon).ok_or_else(invalid)?,
            NaiveDate::from_isoywd_opt(year, week, Weekday::Sun).ok_or_else(invalid)?,
        ));
    }
    if let Some(day) = suffix.strip_prefix('D') {
        if day.len() != 3 {
            return Err(invalid());
        }
        let date = NaiveDate::from_yo_opt(year, number(day)?).ok_or_else(invalid)?;
        return Ok((date, date));
    }
    let (month, months) = match suffix.as_bytes() {
        [] | [b'A', b'1'] => (1, 12),
        [b'Q', n @ b'1'..=b'4'] => (u32::from(n - b'1') * 3 + 1, 3),
        [b'S', n @ b'1'..=b'2'] => (u32::from(n - b'1') * 6 + 1, 6),
        [b'T', n @ b'1'..=b'3'] => (u32::from(n - b'1') * 4 + 1, 4),
        [b'M', _, _] => (number(&suffix[1..])?, 1),
        [_, _] => (number(suffix)?, 1),
        _ => return Err(invalid()),
    };
    let start = NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(invalid)?;
    let next = start
        .checked_add_months(chrono::Months::new(months))
        .ok_or_else(invalid)?;
    Ok((start, next.pred_opt().ok_or_else(invalid)?))
}
