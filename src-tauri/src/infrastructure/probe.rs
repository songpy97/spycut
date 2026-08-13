use std::{path::Path, time::Duration};

use serde::Deserialize;
use tokio::time::timeout;

use crate::domain::media::{FrameRate, MediaInfo, VideoCodec};
use crate::infrastructure::tool_locator::media_command;

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("failed to launch ffprobe: {0}")]
    Launch(#[source] std::io::Error),
    #[error("ffprobe timed out")]
    Timeout,
    #[error("ffprobe failed: {0}")]
    Process(String),
    #[error("ffprobe returned invalid JSON: {0}")]
    Json(#[source] serde_json::Error),
    #[error("only MP4 input is supported in V1")]
    UnsupportedContainer,
    #[error("the file has no supported H.264 or H.265 video stream")]
    UnsupportedVideo,
    #[error("the media duration is missing or invalid")]
    InvalidDuration,
    #[error("the video frame rate is missing or invalid")]
    InvalidFrameRate,
}

#[derive(Debug, Deserialize)]
struct ProbeDocument {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: ProbeFormat,
}

#[derive(Debug, Deserialize)]
struct ProbeFormat {
    format_name: String,
    duration: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    pix_fmt: Option<String>,
    bits_per_raw_sample: Option<String>,
    bit_rate: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u16>,
}

pub async fn probe_media(ffprobe: &Path, source: &Path) -> Result<MediaInfo, ProbeError> {
    let output = timeout(
        Duration::from_secs(30),
        media_command(ffprobe)
            .arg("-v")
            .arg("error")
            .arg("-print_format")
            .arg("json")
            .arg("-show_format")
            .arg("-show_streams")
            .arg(source)
            .output(),
    )
    .await
    .map_err(|_| ProbeError::Timeout)?
    .map_err(ProbeError::Launch)?;

    if !output.status.success() {
        return Err(ProbeError::Process(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    parse_probe_json(&output.stdout, source)
}

pub fn parse_probe_json(bytes: &[u8], source: &Path) -> Result<MediaInfo, ProbeError> {
    let document: ProbeDocument = serde_json::from_slice(bytes).map_err(ProbeError::Json)?;
    if source.extension().and_then(|value| value.to_str()) != Some("mp4")
        && source.extension().and_then(|value| value.to_str()) != Some("MP4")
    {
        return Err(ProbeError::UnsupportedContainer);
    }
    if !document
        .format
        .format_name
        .split(',')
        .any(|name| name == "mp4")
    {
        return Err(ProbeError::UnsupportedContainer);
    }

    let duration_us = parse_seconds_to_micros(document.format.duration.as_deref())
        .ok_or(ProbeError::InvalidDuration)?;
    let video_streams = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("video"))
        .collect::<Vec<_>>();
    let audio_streams = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .collect::<Vec<_>>();
    let video = video_streams
        .first()
        .copied()
        .ok_or(ProbeError::UnsupportedVideo)?;
    let video_codec = video
        .codec_name
        .as_deref()
        .and_then(VideoCodec::from_ffprobe)
        .ok_or(ProbeError::UnsupportedVideo)?;
    let average_frame_rate = video.avg_frame_rate.as_deref().and_then(parse_rational);
    let nominal_frame_rate = video.r_frame_rate.as_deref().and_then(parse_rational);
    let frame_rate = average_frame_rate
        .or(nominal_frame_rate)
        .ok_or(ProbeError::InvalidFrameRate)?;
    let variable_frame_rate = match (average_frame_rate, nominal_frame_rate) {
        (Some(average), Some(nominal)) => {
            let reference = nominal.as_f64().max(1.0);
            (average.as_f64() - nominal.as_f64()).abs() / reference > 0.01
        }
        _ => false,
    };
    let audio = audio_streams.first().copied();

    Ok(MediaInfo {
        duration_us,
        container: "mp4".to_string(),
        video_codec,
        width: video.width.unwrap_or_default(),
        height: video.height.unwrap_or_default(),
        frame_rate,
        variable_frame_rate,
        video_stream_count: u16::try_from(video_streams.len()).unwrap_or(u16::MAX),
        audio_stream_count: u16::try_from(audio_streams.len()).unwrap_or(u16::MAX),
        pixel_format: video.pix_fmt.clone(),
        bit_depth: parse_u8(video.bits_per_raw_sample.as_deref())
            .or_else(|| infer_bit_depth(video.pix_fmt.as_deref())),
        video_bit_rate: parse_u64(video.bit_rate.as_deref())
            .or_else(|| parse_u64(document.format.bit_rate.as_deref())),
        has_audio: audio.is_some(),
        audio_codec: audio.and_then(|value| value.codec_name.clone()),
        audio_sample_rate: audio.and_then(|value| parse_u32(value.sample_rate.as_deref())),
        audio_channels: audio.and_then(|value| value.channels),
        audio_bit_rate: audio.and_then(|value| parse_u64(value.bit_rate.as_deref())),
    })
}

fn parse_seconds_to_micros(value: Option<&str>) -> Option<i64> {
    let seconds = value?.parse::<f64>().ok()?;
    (seconds.is_finite() && seconds > 0.0).then_some((seconds * 1_000_000.0).round() as i64)
}

fn parse_rational(value: &str) -> Option<FrameRate> {
    let (num, den) = value.split_once('/')?;
    FrameRate::new(num.parse().ok()?, den.parse().ok()?)
}

fn parse_u64(value: Option<&str>) -> Option<u64> {
    value?.parse().ok()
}

fn parse_u32(value: Option<&str>) -> Option<u32> {
    value?.parse().ok()
}

fn parse_u8(value: Option<&str>) -> Option<u8> {
    value?.parse().ok()
}

fn infer_bit_depth(pixel_format: Option<&str>) -> Option<u8> {
    let pixel_format = pixel_format?;
    if pixel_format.contains("10") || pixel_format.contains("p010") {
        Some(10)
    } else {
        Some(8)
    }
}
