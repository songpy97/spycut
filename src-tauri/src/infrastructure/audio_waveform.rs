use std::{path::Path, process::Stdio, time::Duration};

use serde::Serialize;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    time::timeout,
};

use crate::domain::time::Micros;
use crate::infrastructure::tool_locator::media_command;

const PCM_SAMPLE_RATE: u32 = 8_000;
pub const WAVEFORM_SAMPLES_PER_SECOND: u32 = 50;
const SAMPLES_PER_BUCKET: usize = (PCM_SAMPLE_RATE / WAVEFORM_SAMPLES_PER_SECOND) as usize;
const MAX_DIAGNOSTIC_BYTES: u64 = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioWaveform {
    pub samples_per_second: u32,
    pub peaks: Vec<u8>,
}

impl AudioWaveform {
    pub fn empty() -> Self {
        Self {
            samples_per_second: WAVEFORM_SAMPLES_PER_SECOND,
            peaks: Vec::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AudioWaveformError {
    #[error("failed to start FFmpeg for waveform analysis: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("failed while reading decoded audio: {0}")]
    Read(#[source] std::io::Error),
    #[error("waveform analysis exceeded its {0}-second time limit")]
    Timeout(u64),
    #[error("FFmpeg waveform analysis failed: {0}")]
    Ffmpeg(String),
    #[error("FFmpeg produced no readable samples for the selected audio stream")]
    Empty,
}

pub async fn extract_audio_waveform(
    ffmpeg: &Path,
    source: &Path,
    duration_us: Micros,
) -> Result<AudioWaveform, AudioWaveformError> {
    let mut child = media_command(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-i"])
        .arg(source)
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-sn",
            "-dn",
            "-ac",
            "1",
            "-ar",
            &PCM_SAMPLE_RATE.to_string(),
            "-acodec",
            "pcm_s16le",
            "-f",
            "s16le",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(AudioWaveformError::Spawn)?;

    let mut stdout = child
        .stdout
        .take()
        .expect("waveform FFmpeg stdout was configured as piped");
    let stderr = child
        .stderr
        .take()
        .expect("waveform FFmpeg stderr was configured as piped");
    let diagnostic_task = tokio::spawn(read_diagnostic(stderr));
    let limit = waveform_timeout(duration_us);
    let mut aggregator = PeakAggregator::new(SAMPLES_PER_BUCKET);

    let decode = async {
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let count = stdout.read(&mut buffer).await?;
            if count == 0 {
                break;
            }
            aggregator.push_bytes(&buffer[..count]);
        }
        child.wait().await
    };

    let status = match timeout(limit, decode).await {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => {
            terminate_child(&mut child).await;
            diagnostic_task.abort();
            let _ = diagnostic_task.await;
            return Err(AudioWaveformError::Read(error));
        }
        Err(_) => {
            terminate_child(&mut child).await;
            diagnostic_task.abort();
            let _ = diagnostic_task.await;
            return Err(AudioWaveformError::Timeout(limit.as_secs()));
        }
    };
    let diagnostic = diagnostic_task.await.unwrap_or_default();
    if !status.success() {
        return Err(AudioWaveformError::Ffmpeg(if diagnostic.is_empty() {
            format!("process exited with status {status}")
        } else {
            diagnostic
        }));
    }

    let peaks = aggregator.finish();
    if peaks.is_empty() {
        return Err(AudioWaveformError::Empty);
    }
    Ok(AudioWaveform {
        samples_per_second: WAVEFORM_SAMPLES_PER_SECOND,
        peaks,
    })
}

async fn read_diagnostic(reader: impl AsyncRead + Unpin) -> String {
    let mut bytes = Vec::new();
    let _ = reader
        .take(MAX_DIAGNOSTIC_BYTES)
        .read_to_end(&mut bytes)
        .await;
    String::from_utf8_lossy(&bytes).trim().to_string()
}

async fn terminate_child(child: &mut tokio::process::Child) {
    let _ = child.start_kill();
    let _ = timeout(Duration::from_secs(5), child.wait()).await;
}

fn waveform_timeout(duration_us: Micros) -> Duration {
    let source_seconds = duration_us.max(0) as u64 / 1_000_000;
    Duration::from_secs((30 + source_seconds / 50).clamp(30, 180))
}

struct PeakAggregator {
    samples_per_bucket: usize,
    samples_in_bucket: usize,
    bucket_peak: u16,
    pending_low_byte: Option<u8>,
    peaks: Vec<u8>,
}

impl PeakAggregator {
    fn new(samples_per_bucket: usize) -> Self {
        assert!(samples_per_bucket > 0);
        Self {
            samples_per_bucket,
            samples_in_bucket: 0,
            bucket_peak: 0,
            pending_low_byte: None,
            peaks: Vec::new(),
        }
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        let mut offset = 0;
        if let Some(low) = self.pending_low_byte.take() {
            if let Some(&high) = bytes.first() {
                self.push_sample(i16::from_le_bytes([low, high]));
                offset = 1;
            } else {
                self.pending_low_byte = Some(low);
                return;
            }
        }

        let pairs = bytes[offset..].chunks_exact(2);
        let remainder = pairs.remainder();
        for pair in pairs {
            self.push_sample(i16::from_le_bytes([pair[0], pair[1]]));
        }
        self.pending_low_byte = remainder.first().copied();
    }

    fn push_sample(&mut self, sample: i16) {
        self.bucket_peak = self.bucket_peak.max(sample.unsigned_abs());
        self.samples_in_bucket += 1;
        if self.samples_in_bucket == self.samples_per_bucket {
            self.flush_bucket();
        }
    }

    fn flush_bucket(&mut self) {
        self.peaks.push(scale_peak(self.bucket_peak));
        self.samples_in_bucket = 0;
        self.bucket_peak = 0;
    }

    fn finish(mut self) -> Vec<u8> {
        if self.samples_in_bucket > 0 {
            self.flush_bucket();
        }
        self.peaks
    }
}

fn scale_peak(peak: u16) -> u8 {
    ((f64::from(peak) / 32_768.0).sqrt() * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use std::{mem::size_of_val, process::Stdio};

    use tempfile::tempdir;
    use tokio::{process::Command, time::timeout};

    use crate::infrastructure::tool_locator::{MediaTool, locate_media_tool};

    use super::*;

    #[test]
    fn waveform_extraction_future_stays_small() {
        let future = extract_audio_waveform(
            Path::new("ffmpeg"),
            Path::new("waveform-source.mp4"),
            1_000_000,
        );

        assert!(
            size_of_val(&future) <= 4 * 1024,
            "waveform extraction future is {} bytes and risks overflowing the Windows IPC thread stack",
            size_of_val(&future)
        );
    }

    #[test]
    fn aggregates_pcm_into_twenty_millisecond_peak_buckets() {
        let mut aggregator = PeakAggregator::new(4);
        aggregator.push_bytes(&[0, 0, 0xff, 0x7f, 0, 0, 0, 0]);
        aggregator.push_bytes(&[0, 0, 0, 0, 0, 0, 0, 0x40]);

        let peaks = aggregator.finish();
        assert_eq!(peaks.len(), 2);
        assert_eq!(peaks[0], u8::MAX);
        assert!(peaks[1] >= 179 && peaks[1] <= 181);
    }

    #[test]
    fn preserves_a_sample_split_across_stream_reads() {
        let mut aggregator = PeakAggregator::new(2);
        aggregator.push_bytes(&[0x00]);
        aggregator.push_bytes(&[0x40, 0x00, 0x00]);

        let peaks = aggregator.finish();
        assert_eq!(peaks.len(), 1);
        assert!(peaks[0] >= 179 && peaks[0] <= 181);
    }

    #[test]
    fn emits_a_partial_final_bucket_and_keeps_silence_at_zero() {
        let mut aggregator = PeakAggregator::new(4);
        aggregator.push_bytes(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

        assert_eq!(aggregator.finish(), vec![0, 0]);
    }

    #[test]
    fn waveform_analysis_timeout_is_finite_and_bounded() {
        assert_eq!(waveform_timeout(1_000_000), Duration::from_secs(30));
        assert_eq!(
            waveform_timeout(24 * 60 * 60 * 1_000_000),
            Duration::from_secs(180)
        );
    }

    #[tokio::test]
    #[ignore = "requires an installed or explicitly configured FFmpeg"]
    async fn extracts_peaks_from_a_real_mp4_audio_stream() {
        let ffmpeg = locate_media_tool(MediaTool::Ffmpeg).unwrap();
        let directory = tempdir().unwrap();
        let source = directory.path().join("waveform-fixture.mp4");
        let generated = timeout(
            Duration::from_secs(30),
            Command::new(&ffmpeg)
                .args([
                    "-hide_banner",
                    "-nostdin",
                    "-loglevel",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:sample_rate=48000",
                    "-t",
                    "1",
                    "-c:a",
                    "aac",
                    "-y",
                ])
                .arg(&source)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .expect("fixture generation timed out")
        .expect("fixture FFmpeg failed to start");
        assert!(
            generated.status.success(),
            "{}",
            String::from_utf8_lossy(&generated.stderr)
        );

        let waveform = extract_audio_waveform(&ffmpeg, &source, 1_000_000)
            .await
            .unwrap();
        assert_eq!(waveform.samples_per_second, 50);
        assert!((48..=52).contains(&waveform.peaks.len()));
        assert!(waveform.peaks.iter().copied().max().unwrap_or_default() > 40);
    }
}
