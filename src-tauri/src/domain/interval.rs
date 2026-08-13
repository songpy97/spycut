use serde::{Deserialize, Serialize};

use super::time::{Micros, TimeRange};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteInterval {
    pub id: u64,
    pub start_us: Micros,
    pub end_us: Micros,
}

impl DeleteInterval {
    pub fn new(id: u64, start_us: Micros, end_us: Micros) -> Result<Self, IntervalError> {
        validate_bounds(start_us, end_us)?;
        Ok(Self {
            id,
            start_us,
            end_us,
        })
    }

    pub fn duration_us(&self) -> Micros {
        self.end_us - self.start_us
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IntervalError {
    #[error("media duration must be positive")]
    InvalidDuration,
    #[error("delete interval start cannot be negative")]
    NegativeStart,
    #[error("delete interval end must be later than its start")]
    EmptyOrReversed,
    #[error("delete interval starts outside the source duration")]
    StartsAfterSource,
    #[error("delete interval {0} does not exist")]
    NotFound(u64),
    #[error("deleting the full source would produce an empty export")]
    EmptyExport,
}

fn validate_bounds(start_us: Micros, end_us: Micros) -> Result<(), IntervalError> {
    if start_us < 0 {
        return Err(IntervalError::NegativeStart);
    }
    if end_us <= start_us {
        return Err(IntervalError::EmptyOrReversed);
    }
    Ok(())
}

pub fn normalize_intervals(
    intervals: &[DeleteInterval],
    duration_us: Micros,
) -> Result<Vec<DeleteInterval>, IntervalError> {
    if duration_us <= 0 {
        return Err(IntervalError::InvalidDuration);
    }

    let mut clamped = Vec::with_capacity(intervals.len());
    for interval in intervals {
        validate_bounds(interval.start_us, interval.end_us)?;
        if interval.start_us >= duration_us {
            return Err(IntervalError::StartsAfterSource);
        }
        clamped.push(DeleteInterval {
            id: interval.id,
            start_us: interval.start_us,
            end_us: interval.end_us.min(duration_us),
        });
    }

    clamped.sort_by_key(|item| (item.start_us, item.end_us, item.id));

    let mut normalized: Vec<DeleteInterval> = Vec::with_capacity(clamped.len());
    for interval in clamped {
        match normalized.last_mut() {
            Some(previous) if interval.start_us <= previous.end_us => {
                previous.end_us = previous.end_us.max(interval.end_us);
                previous.id = previous.id.min(interval.id);
            }
            _ => normalized.push(interval),
        }
    }

    Ok(normalized)
}

pub fn complement_intervals(
    delete_intervals: &[DeleteInterval],
    duration_us: Micros,
) -> Result<Vec<TimeRange>, IntervalError> {
    let normalized = normalize_intervals(delete_intervals, duration_us)?;
    let mut keep = Vec::with_capacity(normalized.len() + 1);
    let mut cursor = 0;

    for interval in normalized {
        if cursor < interval.start_us {
            keep.push(TimeRange {
                start_us: cursor,
                end_us: interval.start_us,
            });
        }
        cursor = interval.end_us;
    }

    if cursor < duration_us {
        keep.push(TimeRange {
            start_us: cursor,
            end_us: duration_us,
        });
    }

    Ok(keep)
}

pub fn resize_interval(
    intervals: &[DeleteInterval],
    id: u64,
    start_us: Micros,
    end_us: Micros,
    duration_us: Micros,
) -> Result<Vec<DeleteInterval>, IntervalError> {
    validate_bounds(start_us, end_us)?;
    let mut updated = intervals.to_vec();
    let interval = updated
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or(IntervalError::NotFound(id))?;
    interval.start_us = start_us;
    interval.end_us = end_us;
    normalize_intervals(&updated, duration_us)
}
