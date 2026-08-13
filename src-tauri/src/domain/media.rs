use serde::{Deserialize, Serialize};

use super::time::Micros;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    H264,
    Hevc,
}

impl VideoCodec {
    pub fn from_ffprobe(value: &str) -> Option<Self> {
        match value {
            "h264" => Some(Self::H264),
            "hevc" | "h265" => Some(Self::Hevc),
            _ => None,
        }
    }

    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::Hevc => "hevc",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

impl FrameRate {
    pub fn new(num: u32, den: u32) -> Option<Self> {
        (num > 0 && den > 0).then_some(Self { num, den })
    }

    pub fn as_f64(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }

    pub fn frame_duration_us(self) -> Micros {
        ((1_000_000_u64 * u64::from(self.den)) / u64::from(self.num)) as Micros
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub duration_us: Micros,
    pub container: String,
    pub video_codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    #[serde(default)]
    pub variable_frame_rate: bool,
    #[serde(default)]
    pub video_stream_count: u16,
    #[serde(default)]
    pub audio_stream_count: u16,
    pub pixel_format: Option<String>,
    pub bit_depth: Option<u8>,
    pub video_bit_rate: Option<u64>,
    pub has_audio: bool,
    pub audio_codec: Option<String>,
    pub audio_sample_rate: Option<u32>,
    pub audio_channels: Option<u16>,
    pub audio_bit_rate: Option<u64>,
}
