use std::{fs, path::Path};

use pretty_assertions::assert_eq;
use spycut_lib::{
    application::session::ProjectSession,
    domain::{
        media::{FrameRate, MediaInfo, VideoCodec},
        project::{PROJECT_SCHEMA_VERSION, ProjectV1, SourceIdentity},
    },
    infrastructure::{
        fingerprint::{fingerprint_source, source_identity_matches},
        probe::{ProbeError, parse_probe_json},
        project_store::{ProjectStore, ProjectStoreError},
    },
};

#[test]
fn ffprobe_json_is_reduced_to_the_supported_media_model() {
    let json = br#"{
      "streams": [
        {
          "codec_type": "video",
          "codec_name": "hevc",
          "width": 3840,
          "height": 2160,
          "avg_frame_rate": "30000/1001",
          "r_frame_rate": "30000/1001",
          "pix_fmt": "yuv420p10le",
          "bits_per_raw_sample": "10",
          "bit_rate": "8000000"
        },
        {
          "codec_type": "audio",
          "codec_name": "aac",
          "sample_rate": "48000",
          "channels": 2,
          "bit_rate": "128000"
        }
      ],
      "format": {
        "format_name": "mov,mp4,m4a,3gp,3g2,mj2",
        "duration": "12.345678",
        "bit_rate": "8200000"
      }
    }"#;

    let info = parse_probe_json(json, Path::new("课程 01.mp4")).unwrap();
    assert_eq!(info.duration_us, 12_345_678);
    assert_eq!(info.video_codec, VideoCodec::Hevc);
    assert_eq!(info.frame_rate, FrameRate::new(30_000, 1_001).unwrap());
    assert_eq!(info.bit_depth, Some(10));
    assert_eq!(info.audio_codec.as_deref(), Some("aac"));
    assert!(!info.variable_frame_rate);
    assert_eq!(info.video_stream_count, 1);
    assert_eq!(info.audio_stream_count, 1);
}

#[test]
fn vfr_and_multiple_streams_are_exposed_for_review_warnings() {
    let json = br#"{
      "streams": [
        {"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"avg_frame_rate":"24000/1001","r_frame_rate":"30/1"},
        {"codec_type":"video","codec_name":"h264","width":320,"height":180,"avg_frame_rate":"30/1","r_frame_rate":"30/1"},
        {"codec_type":"audio","codec_name":"aac","sample_rate":"48000","channels":2},
        {"codec_type":"audio","codec_name":"aac","sample_rate":"48000","channels":2}
      ],
      "format":{"format_name":"mov,mp4","duration":"10"}
    }"#;
    let info = parse_probe_json(json, Path::new("多轨 VFR.mp4")).unwrap();
    assert!(info.variable_frame_rate);
    assert_eq!(info.frame_rate, FrameRate::new(24_000, 1_001).unwrap());
    assert_eq!(info.video_stream_count, 2);
    assert_eq!(info.audio_stream_count, 2);
}

#[test]
fn unsupported_container_and_codec_are_explicit() {
    let webm = br#"{
      "streams": [{"codec_type":"video","codec_name":"vp9","avg_frame_rate":"30/1"}],
      "format": {"format_name":"matroska,webm","duration":"10"}
    }"#;
    assert!(matches!(
        parse_probe_json(webm, Path::new("file.webm")),
        Err(ProbeError::UnsupportedContainer)
    ));

    let vp9_mp4 = br#"{
      "streams": [{"codec_type":"video","codec_name":"vp9","avg_frame_rate":"30/1"}],
      "format": {"format_name":"mov,mp4","duration":"10"}
    }"#;
    assert!(matches!(
        parse_probe_json(vp9_mp4, Path::new("file.mp4")),
        Err(ProbeError::UnsupportedVideo)
    ));
}

#[test]
fn source_fingerprint_detects_edge_changes() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("课程 '01'.mp4");
    fs::write(&source, vec![1_u8; 2 * 1024 * 1024 + 32]).unwrap();
    let first = fingerprint_source(&source).unwrap();
    let same = fingerprint_source(&source).unwrap();
    assert!(source_identity_matches(&first, &same));

    let mut changed = vec![1_u8; 2 * 1024 * 1024 + 32];
    let final_index = changed.len() - 1;
    changed[final_index] = 2;
    fs::write(&source, changed).unwrap();
    let second = fingerprint_source(&source).unwrap();
    assert!(!source_identity_matches(&first, &second));
}

#[test]
fn project_is_atomically_saved_and_loaded() {
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::new(directory.path().join("app-data")).unwrap();
    let source_path = directory.path().join("课程01.mp4");
    let mut project = project(&source_path);
    store.save(&project).unwrap();
    assert!(ProjectStore::sidecar_path(&source_path).unwrap().is_file());
    assert_eq!(store.load(&project.project_id).unwrap(), project);

    project.last_playhead_us = 42;
    store.save(&project).unwrap();
    assert_eq!(store.load(&project.project_id).unwrap(), project);
}

#[test]
fn unknown_schema_is_rejected_without_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::new(directory.path().join("app-data")).unwrap();
    let mut project = project(&directory.path().join("课程01.mp4"));
    project.schema_version = PROJECT_SCHEMA_VERSION + 1;
    store.save(&project).unwrap();

    assert!(matches!(
        store.load(&project.project_id),
        Err(ProjectStoreError::UnsupportedSchema(_))
    ));
}

#[test]
fn session_edits_save_normalized_state_and_support_undo() {
    let mut session = ProjectSession::new(project(Path::new("/tmp/课程01.mp4")));
    session.add_delete_interval(10, 20).unwrap();
    session.add_delete_interval(20, 30).unwrap();
    let projection = session.projection().unwrap();
    assert_eq!(projection.project.delete_intervals.len(), 1);
    assert_eq!(projection.deleted_duration_us, 20);
    assert!(projection.can_undo);

    assert!(session.undo());
    assert_eq!(session.projection().unwrap().deleted_duration_us, 10);
    assert!(session.redo());
    assert_eq!(session.projection().unwrap().deleted_duration_us, 20);
}

#[test]
fn six_hour_project_restores_more_than_one_hundred_intervals_and_playhead() {
    let directory = tempfile::tempdir().unwrap();
    let store = ProjectStore::new(directory.path().join("app-data")).unwrap();
    let source_path = directory.path().join("六小时课程.mp4");
    let mut long_project = project(&source_path);
    long_project.media.duration_us = 6 * 60 * 60 * 1_000_000;
    let mut session = ProjectSession::new(long_project);
    for index in 0..101_i64 {
        let start = index * 60_000_000 + 5_000_000;
        session
            .add_delete_interval(start, start + 1_000_000)
            .unwrap();
    }
    session.set_playhead(5 * 60 * 60 * 1_000_000);
    let projection = session.projection().unwrap();
    assert_eq!(projection.project.delete_intervals.len(), 101);
    assert!(
        projection
            .project
            .delete_intervals
            .windows(2)
            .all(|pair| pair[0].end_us < pair[1].start_us)
    );
    store.save(&projection.project).unwrap();

    let restored = store
        .find_matching_source(&projection.project.source)
        .unwrap()
        .unwrap();
    assert_eq!(
        restored.delete_intervals,
        projection.project.delete_intervals
    );
    assert_eq!(restored.last_playhead_us, 5 * 60 * 60 * 1_000_000);
    assert_eq!(
        ProjectSession::new(restored)
            .projection()
            .unwrap()
            .kept_duration_us,
        projection.kept_duration_us
    );
}

fn project(source_path: &Path) -> ProjectV1 {
    ProjectV1::new(
        SourceIdentity {
            canonical_path: source_path.to_string_lossy().into_owned(),
            size_bytes: 100,
            modified_unix_ms: 1,
            edge_hash_blake3: "hash".to_string(),
        },
        MediaInfo {
            duration_us: 100,
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
        },
    )
}
