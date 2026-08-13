use serde::{Deserialize, Serialize};

pub type Micros = i64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeRange {
    pub start_us: Micros,
    pub end_us: Micros,
}

impl TimeRange {
    pub fn new(start_us: Micros, end_us: Micros) -> Result<Self, TimeRangeError> {
        if start_us < 0 {
            return Err(TimeRangeError::NegativeStart);
        }
        if end_us <= start_us {
            return Err(TimeRangeError::EmptyOrReversed);
        }
        Ok(Self { start_us, end_us })
    }

    pub fn duration_us(self) -> Micros {
        self.end_us - self.start_us
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TimeRangeError {
    #[error("start time cannot be negative")]
    NegativeStart,
    #[error("end time must be later than start time")]
    EmptyOrReversed,
}
