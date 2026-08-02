use std::{
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, NaiveDate, NaiveDateTime};

#[derive(Clone, Copy, Debug)]
pub struct Since(i64);

impl Since {
    pub fn timestamp(self) -> i64 {
        self.0
    }
}

impl FromStr for Since {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Some(relative) = value.strip_prefix('-') {
            return parse_relative(relative).map(Self);
        }

        if let Ok(timestamp) = value.parse::<i64>() {
            return Ok(Self(timestamp));
        }

        if let Ok(duration) = humantime::parse_duration(value) {
            return timestamp_before_now(duration.as_secs()).map(Self);
        }

        if let Ok(datetime) = DateTime::parse_from_rfc3339(value) {
            return Ok(Self(datetime.timestamp()));
        }

        for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S"] {
            if let Ok(datetime) = NaiveDateTime::parse_from_str(value, format) {
                return Ok(Self(datetime.and_utc().timestamp()));
            }
        }

        if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
            return Ok(Self(
                date.and_hms_opt(0, 0, 0)
                    .expect("midnight is valid")
                    .and_utc()
                    .timestamp(),
            ));
        }

        Err(format!(
            "invalid time `{value}`; use Unix seconds, ISO-8601, or a duration such as 7d"
        ))
    }
}

fn parse_relative(value: &str) -> Result<i64, String> {
    if value.is_empty() {
        return Err("relative time must include a duration, such as -1h".into());
    }

    let seconds = match value.parse::<u64>() {
        Ok(seconds) => seconds,
        Err(_) => humantime::parse_duration(value)
            .map_err(|error| format!("invalid relative duration `-{value}`: {error}"))?
            .as_secs(),
    };
    timestamp_before_now(seconds)
}

fn timestamp_before_now(seconds: u64) -> Result<i64, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
        .as_secs();
    let timestamp = now
        .checked_sub(seconds)
        .ok_or_else(|| "relative time is before the Unix epoch".to_string())?;

    i64::try_from(timestamp).map_err(|_| "relative timestamp is too large".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn parses_unix_timestamp() {
        assert_eq!(
            Since::from_str("1773703418").unwrap().timestamp(),
            1773703418
        );
    }

    #[test]
    fn parses_iso_timestamps() {
        let expected = DateTime::parse_from_rfc3339("2026-03-17T04:43:38Z")
            .unwrap()
            .timestamp();
        assert_eq!(
            Since::from_str("2026-03-17T04:43:38Z").unwrap().timestamp(),
            expected
        );
        assert_eq!(
            Since::from_str("2026-03-17").unwrap().timestamp(),
            NaiveDate::from_ymd_opt(2026, 3, 17)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc()
                .timestamp()
        );
    }

    #[test]
    fn parses_relative_duration() {
        let before = Utc::now().timestamp();
        let parsed = Since::from_str("2h").unwrap().timestamp();
        let after = Utc::now().timestamp();
        assert!((before - 7200..=after - 7200).contains(&parsed));
    }

    #[test]
    fn parses_negative_integer_as_relative_seconds() {
        let before = Utc::now().timestamp();
        let parsed = Since::from_str("-30").unwrap().timestamp();
        let after = Utc::now().timestamp();
        assert!((before - 30..=after - 30).contains(&parsed));
    }
}
