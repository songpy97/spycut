use std::{path::Path, process::Stdio, time::Duration};

use serde::Serialize;
use tokio::time::timeout;

use crate::domain::media::{MediaInfo, VideoCodec};
use crate::infrastructure::tool_locator::media_command;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncoderSelection {
    pub name: String,
    pub hardware_accelerated: bool,
    pub display_name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum EncoderError {
    #[error("failed to query FFmpeg encoders: {0}")]
    Query(#[source] std::io::Error),
    #[error("FFmpeg encoder query timed out")]
    Timeout,
    #[error("no usable {0} encoder is available")]
    Unavailable(&'static str),
}

pub async fn select_encoder(
    ffmpeg: &Path,
    codec: VideoCodec,
) -> Result<EncoderSelection, EncoderError> {
    let output = timeout(
        Duration::from_secs(15),
        media_command(ffmpeg)
            .args(["-hide_banner", "-encoders"])
            .output(),
    )
    .await
    .map_err(|_| EncoderError::Timeout)?
    .map_err(EncoderError::Query)?;
    let listing = String::from_utf8_lossy(&output.stdout);

    for selection in candidates(codec) {
        if listing
            .split_whitespace()
            .any(|token| token == selection.name)
            && encoder_preflight(ffmpeg, &selection).await
        {
            return Ok(selection);
        }
    }

    Err(EncoderError::Unavailable(codec.ffmpeg_name()))
}

pub fn video_arguments(selection: &EncoderSelection, media: &MediaInfo) -> Vec<String> {
    let bit_rate = target_bit_rate(media);
    let name = selection.name.as_str();
    let mut args = vec!["-c:v".into(), selection.name.clone()];
    match name {
        "h264_videotoolbox" | "hevc_videotoolbox" => {
            args.extend([
                "-b:v".into(),
                bit_rate.to_string(),
                "-maxrate".into(),
                (bit_rate * 3 / 2).to_string(),
                "-bufsize".into(),
                (bit_rate * 2).to_string(),
                "-allow_sw".into(),
                "1".into(),
            ]);
        }
        "h264_nvenc" | "hevc_nvenc" => {
            args.extend([
                "-preset".into(),
                "p5".into(),
                "-rc".into(),
                "vbr".into(),
                "-cq".into(),
                "19".into(),
                "-b:v".into(),
                bit_rate.to_string(),
            ]);
        }
        "h264_qsv" | "hevc_qsv" => {
            args.extend([
                "-preset".into(),
                "medium".into(),
                "-global_quality".into(),
                "19".into(),
            ]);
        }
        "h264_amf" | "hevc_amf" => {
            args.extend([
                "-quality".into(),
                "quality".into(),
                "-rc".into(),
                "cqp".into(),
            ]);
        }
        "h264_mf" | "hevc_mf" => {
            args.extend([
                "-rate_control".into(),
                "quality".into(),
                "-quality".into(),
                "80".into(),
            ]);
        }
        "libx264" => {
            args.extend(["-preset".into(), "fast".into(), "-crf".into(), "18".into()]);
        }
        "libx265" => {
            args.extend(["-preset".into(), "fast".into(), "-crf".into(), "20".into()]);
        }
        _ => {}
    }
    if media.video_codec == VideoCodec::Hevc {
        args.extend(["-tag:v".into(), "hvc1".into()]);
    }
    args
}

fn target_bit_rate(media: &MediaInfo) -> u64 {
    let inferred = (u64::from(media.width)
        * u64::from(media.height)
        * media.frame_rate.as_f64().round().max(1.0) as u64)
        / if media.video_codec == VideoCodec::Hevc {
            18
        } else {
            12
        };
    media
        .video_bit_rate
        .map(|value| value * 11 / 10)
        .unwrap_or(inferred)
        .clamp(2_000_000, 80_000_000)
}

async fn encoder_preflight(ffmpeg: &Path, selection: &EncoderSelection) -> bool {
    let mut command = media_command(ffmpeg);
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=128x128:r=30:d=0.1",
            "-frames:v",
            "1",
            "-an",
            "-c:v",
            &selection.name,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if selection.name.starts_with("hevc_") || selection.name == "libx265" {
        command.args(["-tag:v", "hvc1"]);
    }
    command.args(["-f", "null", "-"]);
    matches!(
        timeout(Duration::from_secs(15), command.status()).await,
        Ok(Ok(status)) if status.success()
    )
}

fn candidates(codec: VideoCodec) -> Vec<EncoderSelection> {
    let names: &[(&str, bool, &str)] = match codec {
        VideoCodec::H264 => &[
            #[cfg(target_os = "macos")]
            ("h264_videotoolbox", true, "Apple VideoToolbox H.264"),
            #[cfg(target_os = "windows")]
            ("h264_nvenc", true, "NVIDIA NVENC H.264"),
            #[cfg(target_os = "windows")]
            ("h264_qsv", true, "Intel Quick Sync H.264"),
            #[cfg(target_os = "windows")]
            ("h264_amf", true, "AMD AMF H.264"),
            #[cfg(target_os = "windows")]
            ("h264_mf", false, "Windows Media Foundation H.264"),
            ("libx264", false, "x264 软件编码"),
        ],
        VideoCodec::Hevc => &[
            #[cfg(target_os = "macos")]
            ("hevc_videotoolbox", true, "Apple VideoToolbox H.265"),
            #[cfg(target_os = "windows")]
            ("hevc_nvenc", true, "NVIDIA NVENC H.265"),
            #[cfg(target_os = "windows")]
            ("hevc_qsv", true, "Intel Quick Sync H.265"),
            #[cfg(target_os = "windows")]
            ("hevc_amf", true, "AMD AMF H.265"),
            #[cfg(target_os = "windows")]
            ("hevc_mf", false, "Windows Media Foundation H.265"),
            ("libx265", false, "x265 软件编码"),
        ],
    };
    names
        .iter()
        .map(
            |(name, hardware_accelerated, display_name)| EncoderSelection {
                name: (*name).into(),
                hardware_accelerated: *hardware_accelerated,
                display_name: (*display_name).into(),
            },
        )
        .collect()
}
