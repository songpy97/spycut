use crate::domain::{export_plan::ExportPlan, time::TimeRange};

#[derive(Debug, thiserror::Error)]
pub enum FilterScriptError {
    #[error("the export plan contains no keep intervals")]
    NoKeepIntervals,
}

pub fn build_filter_script(plan: &ExportPlan) -> Result<String, FilterScriptError> {
    if plan.keep_intervals.is_empty() {
        return Err(FilterScriptError::NoKeepIntervals);
    }

    let expression = plan
        .keep_intervals
        .iter()
        .map(keep_expression)
        .collect::<Vec<_>>()
        .join("+");
    let frame_rate = format!(
        "{}/{}",
        plan.output_frame_rate.num, plan.output_frame_rate.den
    );
    let video_pts = compacted_video_pts(plan);

    let mut script = format!(
        "[0:v:0]split=2[vkeep][vscan];[vkeep]select='{expression}',settb=AVTB,setpts='{video_pts}',fps={frame_rate}:start_time=0,format=yuv420p[vout];[vscan]fps=1/2,setpts=PTS-STARTPTS[vprogress]"
    );
    if plan.has_audio {
        // Splitting audio frames into small packets before aselect keeps boundary error below
        // one millisecond at common 44.1/48 kHz sample rates without creating huge graphs.
        script.push_str(&format!(
            ";[0:a:0]asetnsamples=n=32:p=0,aselect='{expression}',asettb=AVTB,asetpts=N/SR/TB[aout]"
        ));
    }
    Ok(script)
}

fn compacted_video_pts(plan: &ExportPlan) -> String {
    let first_keep_start = plan.keep_intervals[0].start_us as f64 / 1_000_000.0;
    let source_time = format!("T-STARTT+{first_keep_start:.6}");
    if plan.delete_intervals.is_empty() {
        return format!("({source_time})/TB");
    }
    let deleted_before_pts = plan
        .delete_intervals
        .iter()
        .map(|interval| {
            format!(
                "gte({source_time},{:.6})*{:.6}",
                interval.end_us as f64 / 1_000_000.0,
                interval.duration_us() as f64 / 1_000_000.0
            )
        })
        .collect::<Vec<_>>()
        .join("+");
    format!("({source_time}-({deleted_before_pts}))/TB")
}

fn keep_expression(range: &TimeRange) -> String {
    format!(
        "gte(t,{:.6})*lt(t,{:.6})",
        range.start_us as f64 / 1_000_000.0,
        range.end_us as f64 / 1_000_000.0
    )
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        export_plan::ExportPlan,
        interval::DeleteInterval,
        media::{FrameRate, MediaInfo, VideoCodec},
    };

    use super::*;

    fn media(has_audio: bool) -> MediaInfo {
        MediaInfo {
            duration_us: 10_000_000,
            container: "mp4".into(),
            video_codec: VideoCodec::H264,
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            variable_frame_rate: false,
            video_stream_count: 1,
            audio_stream_count: u16::from(has_audio),
            pixel_format: Some("yuv420p".into()),
            bit_depth: Some(8),
            video_bit_rate: Some(8_000_000),
            has_audio,
            audio_codec: has_audio.then(|| "aac".into()),
            audio_sample_rate: has_audio.then_some(48_000),
            audio_channels: has_audio.then_some(2),
            audio_bit_rate: has_audio.then_some(160_000),
        }
    }

    #[test]
    fn creates_sequential_video_and_audio_timestamps() {
        let plan = ExportPlan::build(
            &media(true),
            &[DeleteInterval::new(1, 2_000_000, 4_500_000).unwrap()],
        )
        .unwrap();
        let script = build_filter_script(&plan).unwrap();

        assert!(script.contains("gte(t,0.000000)*lt(t,2.000000)"));
        assert!(script.contains("gte(t,4.500000)*lt(t,10.000000)"));
        assert!(script.contains("[vkeep]select="));
        assert!(
            script
                .contains(",settb=AVTB,setpts='(T-STARTT+0.000000-(gte(T-STARTT+0.000000,4.500000)*2.500000))/TB',fps=30/1:start_time=0")
        );
        assert!(!script.contains("[vkeep]setpts=PTS-STARTPTS,select="));
        assert!(!script.contains("setpts=N/("));
        assert!(script.contains("split=2[vkeep][vscan]"));
        assert!(script.contains("fps=1/2"));
        assert!(!script.contains("[0:a:0]asetpts=PTS-STARTPTS"));
        assert!(script.contains("[0:a:0]asetnsamples=n=32:p=0,aselect=",));
        assert!(script.contains(",asettb=AVTB,asetpts=N/SR/TB[aout]"));
    }

    #[test]
    fn compacts_video_pts_by_all_completed_delete_intervals() {
        let plan = ExportPlan::build(
            &media(false),
            &[
                DeleteInterval::new(1, 2_000_000, 4_500_000).unwrap(),
                DeleteInterval::new(2, 6_000_000, 7_000_000).unwrap(),
            ],
        )
        .unwrap();
        let script = build_filter_script(&plan).unwrap();

        assert!(script.contains(
            "setpts='(T-STARTT+0.000000-(gte(T-STARTT+0.000000,4.500000)*2.500000+gte(T-STARTT+0.000000,7.000000)*1.000000))/TB'"
        ));
    }

    #[test]
    fn initial_delete_still_rebases_the_first_kept_frame_to_zero() {
        let plan = ExportPlan::build(
            &media(false),
            &[DeleteInterval::new(1, 0, 5_000_000).unwrap()],
        )
        .unwrap();
        let script = build_filter_script(&plan).unwrap();

        assert!(script.contains(
            "setpts='(T-STARTT+5.000000-(gte(T-STARTT+5.000000,5.000000)*5.000000))/TB'"
        ));
    }

    #[test]
    fn omits_audio_graph_for_silent_sources() {
        let plan = ExportPlan::build(
            &media(false),
            &[DeleteInterval::new(1, 1_000_000, 2_000_000).unwrap()],
        )
        .unwrap();
        let script = build_filter_script(&plan).unwrap();
        assert!(!script.contains("[0:a:0]"));
    }
}
