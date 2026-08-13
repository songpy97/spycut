use serde::{Deserialize, Serialize};

use super::{
    interval::{DeleteInterval, IntervalError, complement_intervals, normalize_intervals},
    media::{FrameRate, MediaInfo, VideoCodec},
    time::{Micros, TimeRange},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPlan {
    pub source_duration_us: Micros,
    pub deleted_duration_us: Micros,
    pub kept_duration_us: Micros,
    pub delete_intervals: Vec<DeleteInterval>,
    pub keep_intervals: Vec<TimeRange>,
    pub output_codec: VideoCodec,
    pub output_frame_rate: FrameRate,
    pub has_audio: bool,
}

impl ExportPlan {
    pub fn build(
        media: &MediaInfo,
        delete_intervals: &[DeleteInterval],
    ) -> Result<Self, IntervalError> {
        let normalized = normalize_intervals(delete_intervals, media.duration_us)?;
        let keep = complement_intervals(&normalized, media.duration_us)?;
        if keep.is_empty() {
            return Err(IntervalError::EmptyExport);
        }
        let deleted_duration_us = normalized.iter().map(DeleteInterval::duration_us).sum();
        let kept_duration_us = keep.iter().copied().map(TimeRange::duration_us).sum();

        Ok(Self {
            source_duration_us: media.duration_us,
            deleted_duration_us,
            kept_duration_us,
            delete_intervals: normalized,
            keep_intervals: keep,
            output_codec: media.video_codec,
            output_frame_rate: media.frame_rate,
            has_audio: media.has_audio,
        })
    }
}
