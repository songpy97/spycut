use serde::Serialize;

use crate::domain::time::Micros;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub job_id: String,
    pub phase: ExportPhase,
    pub percent: f64,
    pub processed_source_us: Micros,
    pub source_duration_us: Micros,
    pub speed: Option<String>,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportPhase {
    Preparing,
    Encoding,
    Validating,
    Finalizing,
}

#[derive(Default)]
pub struct ProgressParser {
    out_time_us: Micros,
    speed: Option<String>,
}

impl ProgressParser {
    pub fn push_line(&mut self, line: &str) -> bool {
        let Some((key, value)) = line.trim().split_once('=') else {
            return false;
        };
        match key {
            "out_time_us" => self.out_time_us = value.parse().unwrap_or(self.out_time_us),
            "speed" => self.speed = Some(value.to_string()),
            "progress" => return value == "continue" || value == "end",
            _ => {}
        }
        false
    }

    pub fn report(&self, job_id: &str, source_duration_us: Micros) -> ExportProgress {
        let processed = self.out_time_us.clamp(0, source_duration_us);
        let percent = if source_duration_us > 0 {
            (processed as f64 / source_duration_us as f64 * 94.0).clamp(0.0, 94.0)
        } else {
            0.0
        };
        ExportProgress {
            job_id: job_id.to_string(),
            phase: ExportPhase::Encoding,
            percent,
            processed_source_us: processed,
            source_duration_us,
            speed: self.speed.clone(),
            message: "正在顺序读取源视频并精确重建保留片段".into(),
        }
    }

    pub fn stalled_report(&self, job_id: &str, source_duration_us: Micros) -> ExportProgress {
        let mut report = self.report(job_id, source_duration_us);
        report.message = "超过 60 秒没有收到新进度，任务可能卡住；可以继续等待或取消导出".into();
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ffmpeg_progress_blocks() {
        let mut parser = ProgressParser::default();
        assert!(!parser.push_line("out_time_us=2500000"));
        assert!(!parser.push_line("speed=2.50x"));
        assert!(parser.push_line("progress=continue"));
        let report = parser.report("job", 10_000_000);
        assert_eq!(report.processed_source_us, 2_500_000);
        assert_eq!(report.percent, 23.5);
        assert_eq!(report.speed.as_deref(), Some("2.50x"));
    }

    #[test]
    fn stalled_report_preserves_last_known_progress() {
        let mut parser = ProgressParser::default();
        parser.push_line("out_time_us=2500000");
        let report = parser.stalled_report("job", 10_000_000);
        assert_eq!(report.processed_source_us, 2_500_000);
        assert!(report.message.contains("可能卡住"));
    }
}
