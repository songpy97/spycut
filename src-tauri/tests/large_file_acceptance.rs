use std::{path::PathBuf, time::Instant};

use spycut_lib::infrastructure::{
    fingerprint::fingerprint_source,
    probe::probe_media,
    tool_locator::{MediaTool, locate_media_tool},
};

#[tokio::test]
#[ignore = "requires SPYCUT_LARGE_FIXTURE pointing to a valid 10+ GiB MP4"]
async fn probes_and_fingerprints_a_large_4k_mp4_without_copying_it() {
    let source = PathBuf::from(
        std::env::var_os("SPYCUT_LARGE_FIXTURE")
            .expect("SPYCUT_LARGE_FIXTURE must point to the acceptance fixture"),
    );
    let ffprobe = locate_media_tool(MediaTool::Ffprobe).expect("ffprobe is required");
    let started = Instant::now();
    let media = probe_media(&ffprobe, &source).await.unwrap();
    let identity = fingerprint_source(&source).unwrap();
    let elapsed = started.elapsed();

    assert!(identity.size_bytes > 10 * 1024 * 1024 * 1024_u64);
    assert!(media.width >= 3_840 && media.height >= 2_160);
    assert!(media.duration_us >= 3 * 60 * 60 * 1_000_000);
    assert!(elapsed < std::time::Duration::from_secs(30));
    assert_eq!(source.metadata().unwrap().len(), identity.size_bytes);
}
