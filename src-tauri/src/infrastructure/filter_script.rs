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
        script.push_str(&audio_filter_script(plan));
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
                "min(max({source_time}-{:.6},0),{:.6})",
                interval.start_us as f64 / 1_000_000.0,
                interval.duration_us() as f64 / 1_000_000.0
            )
        })
        .collect::<Vec<_>>()
        .join("+");
    format!("({source_time}-({deleted_before_pts}))/TB")
}

fn audio_filter_script(plan: &ExportPlan) -> String {
    let mut boundaries = plan
        .delete_intervals
        .iter()
        .flat_map(|interval| [interval.start_us, interval.end_us])
        .filter(|timestamp| *timestamp > 0 && *timestamp < plan.source_duration_us)
        .collect::<Vec<_>>();
    boundaries.sort_unstable();
    boundaries.dedup();
    if boundaries.is_empty() {
        return ";[0:a:0]asetnsamples=n=32:p=0,asettb=AVTB,asetpts=N/SR/TB[aout]".into();
    }

    let segment_count = boundaries.len() + 1;
    let segment_outputs = (0..segment_count)
        .map(|index| format!("[apart{index}]"))
        .collect::<String>();
    let timestamps = boundaries
        .iter()
        .map(|timestamp| format!("{:.6}", *timestamp as f64 / 1_000_000.0))
        .collect::<Vec<_>>()
        .join("|");
    let mut script =
        format!(";[0:a:0]asetnsamples=n=32:p=0,asegment=timestamps={timestamps}{segment_outputs}");

    let mut points = Vec::with_capacity(segment_count + 1);
    points.push(0);
    points.extend(boundaries);
    points.push(plan.source_duration_us);
    let mut keep_count = 0;
    for (index, segment) in points.windows(2).enumerate() {
        let is_kept = plan
            .keep_intervals
            .iter()
            .any(|range| range.start_us == segment[0] && range.end_us == segment[1]);
        if is_kept {
            script.push_str(&format!(
                ";[apart{index}]asettb=AVTB,asetpts=N/SR/TB[akeep{keep_count}]"
            ));
            keep_count += 1;
        } else {
            script.push_str(&format!(";[apart{index}]anullsink"));
        }
    }

    if keep_count == 1 {
        script.push_str(";[akeep0]anull[aout]");
        return script;
    }
    let keep_inputs = (0..keep_count)
        .map(|index| format!("[akeep{index}]"))
        .collect::<String>();
    script.push_str(&format!(
        ";{keep_inputs}concat=n={keep_count}:v=0:a=1,asettb=AVTB,asetpts=N/SR/TB[aout]"
    ));
    script
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
                .contains(",settb=AVTB,setpts='(T-STARTT+0.000000-(min(max(T-STARTT+0.000000-2.000000,0),2.500000)))/TB',fps=30/1:start_time=0")
        );
        assert!(!script.contains("[vkeep]setpts=PTS-STARTPTS,select="));
        assert!(!script.contains("setpts=N/("));
        assert!(script.contains("split=2[vkeep][vscan]"));
        assert!(script.contains("fps=1/2"));
        assert!(!script.contains("[0:a:0]asetpts=PTS-STARTPTS"));
        assert!(!script.contains("aselect="));
        assert!(script.contains(
            "[0:a:0]asetnsamples=n=32:p=0,asegment=timestamps=2.000000|4.500000[apart0][apart1][apart2]"
        ));
        assert!(script.contains("[apart0]asettb=AVTB,asetpts=N/SR/TB[akeep0]"));
        assert!(script.contains("[apart1]anullsink"));
        assert!(script.contains("[apart2]asettb=AVTB,asetpts=N/SR/TB[akeep1]"));
        assert!(
            script.contains("[akeep0][akeep1]concat=n=2:v=0:a=1,asettb=AVTB,asetpts=N/SR/TB[aout]")
        );
    }

    #[test]
    fn compacts_video_pts_by_elapsed_delete_overlap() {
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
            "setpts='(T-STARTT+0.000000-(min(max(T-STARTT+0.000000-2.000000,0),2.500000)+min(max(T-STARTT+0.000000-6.000000,0),1.000000)))/TB'"
        ));
    }

    #[test]
    fn trailing_delete_compacts_an_eof_before_the_container_end() {
        let plan = ExportPlan::build(
            &media(false),
            &[DeleteInterval::new(1, 8_000_000, 10_000_000).unwrap()],
        )
        .unwrap();
        let script = build_filter_script(&plan).unwrap();

        assert!(script.contains(
            "setpts='(T-STARTT+0.000000-(min(max(T-STARTT+0.000000-8.000000,0),2.000000)))/TB'"
        ));
        assert!(!script.contains("gte(T-STARTT+0.000000,10.000000)"));
    }

    #[test]
    fn initial_delete_still_rebases_the_first_kept_frame_to_zero() {
        let plan = ExportPlan::build(
            &media(true),
            &[DeleteInterval::new(1, 0, 5_000_000).unwrap()],
        )
        .unwrap();
        let script = build_filter_script(&plan).unwrap();

        assert!(script.contains(
            "setpts='(T-STARTT+5.000000-(min(max(T-STARTT+5.000000-0.000000,0),5.000000)))/TB'"
        ));
        assert!(script.contains(
            "asegment=timestamps=5.000000[apart0][apart1];[apart0]anullsink;[apart1]asettb=AVTB,asetpts=N/SR/TB[akeep0];[akeep0]anull[aout]"
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
