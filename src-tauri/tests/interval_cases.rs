use pretty_assertions::assert_eq;
use spycut_lib::{
    application::history::CommandHistory,
    domain::{
        export_plan::ExportPlan,
        interval::{
            DeleteInterval, IntervalError, complement_intervals, normalize_intervals,
            resize_interval,
        },
        media::{FrameRate, MediaInfo, VideoCodec},
        time::TimeRange,
    },
};

const DURATION: i64 = 100;

fn interval(id: u64, start_us: i64, end_us: i64) -> DeleteInterval {
    DeleteInterval::new(id, start_us, end_us).unwrap()
}

#[test]
fn empty_delete_list_keeps_the_full_source() {
    assert_eq!(
        complement_intervals(&[], DURATION).unwrap(),
        vec![TimeRange {
            start_us: 0,
            end_us: DURATION
        }]
    );
}

#[test]
fn one_middle_delete_produces_two_keeps_in_order() {
    assert_eq!(
        complement_intervals(&[interval(1, 20, 40)], DURATION).unwrap(),
        vec![
            TimeRange {
                start_us: 0,
                end_us: 20
            },
            TimeRange {
                start_us: 40,
                end_us: 100
            }
        ]
    );
}

#[test]
fn edge_deletes_do_not_create_zero_length_keeps() {
    assert_eq!(
        complement_intervals(&[interval(1, 0, 20), interval(2, 80, 100)], DURATION).unwrap(),
        vec![TimeRange {
            start_us: 20,
            end_us: 80
        }]
    );
}

#[test]
fn overlapping_contained_and_adjacent_deletes_merge_deterministically() {
    let normalized = normalize_intervals(
        &[
            interval(9, 30, 50),
            interval(7, 20, 40),
            interval(8, 25, 35),
            interval(6, 50, 60),
        ],
        DURATION,
    )
    .unwrap();

    assert_eq!(normalized, vec![interval(6, 20, 60)]);
}

#[test]
fn intervals_are_clamped_at_the_source_end() {
    assert_eq!(
        normalize_intervals(&[interval(1, 90, 200)], DURATION).unwrap(),
        vec![interval(1, 90, 100)]
    );
}

#[test]
fn invalid_intervals_are_rejected() {
    assert_eq!(
        DeleteInterval::new(1, -1, 5),
        Err(IntervalError::NegativeStart)
    );
    assert_eq!(
        DeleteInterval::new(1, 5, 5),
        Err(IntervalError::EmptyOrReversed)
    );
    assert_eq!(
        normalize_intervals(&[interval(1, 100, 110)], DURATION),
        Err(IntervalError::StartsAfterSource)
    );
}

#[test]
fn resize_cannot_invert_and_missing_id_is_reported() {
    assert_eq!(
        resize_interval(&[interval(1, 20, 30)], 1, 40, 30, DURATION),
        Err(IntervalError::EmptyOrReversed)
    );
    assert_eq!(
        resize_interval(&[interval(1, 20, 30)], 2, 10, 15, DURATION),
        Err(IntervalError::NotFound(2))
    );
}

#[test]
fn deleting_the_full_source_is_not_exportable() {
    let media = media_info();
    assert_eq!(
        ExportPlan::build(&media, &[interval(1, 0, DURATION)]),
        Err(IntervalError::EmptyExport)
    );
}

#[test]
fn one_thousand_intervals_normalize_without_reordering() {
    let intervals: Vec<_> = (0..1_000)
        .rev()
        .map(|index| interval(index + 1, index as i64 * 10, index as i64 * 10 + 5))
        .collect();
    let normalized = normalize_intervals(&intervals, 10_000).unwrap();
    assert_eq!(normalized.len(), 1_000);
    assert!(
        normalized
            .windows(2)
            .all(|pair| pair[0].start_us < pair[1].start_us)
    );
}

#[test]
fn history_is_bounded_and_redo_is_cleared_by_new_work() {
    let mut history = CommandHistory::new(2);
    history.record(&0);
    history.record(&1);
    history.record(&2);

    let first = history.undo(&3).unwrap();
    assert_eq!(first, 2);
    let second = history.undo(&first).unwrap();
    assert_eq!(second, 1);
    assert!(!history.can_undo());

    let redone = history.redo(&second).unwrap();
    assert_eq!(redone, 2);
    history.record(&redone);
    assert!(!history.can_redo());
}

fn media_info() -> MediaInfo {
    MediaInfo {
        duration_us: DURATION,
        container: "mp4".to_string(),
        video_codec: VideoCodec::Hevc,
        width: 1920,
        height: 1080,
        frame_rate: FrameRate::new(30, 1).unwrap(),
        variable_frame_rate: false,
        video_stream_count: 1,
        audio_stream_count: 1,
        pixel_format: Some("yuv420p".to_string()),
        bit_depth: Some(8),
        video_bit_rate: Some(5_000_000),
        has_audio: true,
        audio_codec: Some("aac".to_string()),
        audio_sample_rate: Some(48_000),
        audio_channels: Some(2),
        audio_bit_rate: Some(128_000),
    }
}
