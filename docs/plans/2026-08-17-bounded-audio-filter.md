# Bounded Audio Filter Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Prevent FFmpeg `AVERROR(ENOSPC)` on projects with many delete intervals while preserving frame-accurate audio compaction.

**Architecture:** Replace one wide `asegment` with a chain of two-output `asegment` nodes, then combine kept segments through two-input `concat` nodes. All boundary decisions remain on the decoder's original wall-clock timeline, and final audio PTS remains sample-count based.

**Tech Stack:** Rust, FFmpeg filter graphs, Cargo tests, pnpm verification

---

### Task 1: Lock the bounded topology with failing tests

**Files:**
- Modify: `src-tauri/src/infrastructure/filter_script.rs`

**Step 1: Write the failing test**

Add a 40-delete-interval plan and assert that every `asegment` has one timestamp/two outputs, no pipe-separated timestamp list exists, and no `concat=n` value exceeds 2.

**Step 2: Run the focused test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml infrastructure::filter_script::tests`

Expected: FAIL because v1.1.2 emits one 80-output `asegment` and `concat=n=40`.

### Task 2: Generate a bounded streaming graph

**Files:**
- Modify: `src-tauri/src/infrastructure/filter_script.rs`

**Step 1: Replace wide segmentation**

Generate `asetnsamples` once, then one `asegment=timestamps=<absolute boundary>` per boundary with `[segment][remaining]` outputs. Route each finished segment immediately to `anullsink` or a rebased keep label.

**Step 2: Replace wide concatenation**

Combine kept labels in source order with `concat=n=2:v=0:a=1`; finish with `asettb=AVTB,asetpts=N/SR/TB[aout]`.

**Step 3: Run focused tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml infrastructure::filter_script::tests`

Expected: PASS.

### Task 3: Add and run a real FFmpeg many-interval regression

**Files:**
- Modify: `src-tauri/src/infrastructure/ffmpeg_job.rs`

**Step 1: Add ignored integration coverage**

Generate a short HEVC/AAC fixture, build 40 alternating delete intervals, run the normal export command, and validate output duration and A/V drift.

**Step 2: Run the exact media test**

Run the new ignored test with local `SPYCUT_FFMPEG_PATH` and `SPYCUT_FFPROBE_PATH`.

Expected: PASS without `No space left on device` and with duration/A-V validation inside existing tolerances.

### Task 4: Synchronize stable documentation and verify

**Files:**
- Modify: `AGENTS.md`
- Modify: `docs/SpyCut-V1-开发文档.md`

**Step 1: Document the bounded topology**

Replace the one-shot segmentation example and state that production filters must keep per-node audio fan-in/fan-out bounded.

**Step 2: Run full verification**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`

Run: `pnpm check`

Run: `git diff --check`

Expected: all commands pass and no unrelated files change.
