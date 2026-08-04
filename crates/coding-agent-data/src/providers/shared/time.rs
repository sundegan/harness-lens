use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::Timestamp;

pub(crate) fn parse_rfc3339(value: &str) -> Option<Timestamp> {
    let value = OffsetDateTime::parse(value, &Rfc3339).ok()?;
    let milliseconds = value
        .unix_timestamp()
        .saturating_mul(1000)
        .saturating_add(i64::from(value.nanosecond() / 1_000_000));
    Some(Timestamp::from_millis(milliseconds))
}
