# VFR Export Timebase Regression Repair Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Preserve full wall-clock duration for VFR sources whose decoded tracks use non-standard timestamp timebases.

**Architecture:** Keep selection on the decoder-provided original `t` timeline. After selection, rescale timestamps to `AVTB`, rebuild video PTS from `T/STARTT` minus cumulative deleted seconds, and rebuild audio PTS from retained sample count.

**Tech Stack:** Rust, Tauri 2, FFmpeg/FFprobe filter graphs, Cargo tests

---

### Task 1: Lock the corrected graph ordering

**Files:**
- Modify: `src-tauri/src/infrastructure/filter_script.rs`

**Step 1: Write failing assertions**

Require video ordering `select -> settb=AVTB -> setpts -> fps` and audio ordering `asetnsamples -> aselect -> asettb=AVTB -> asetpts`. Reject `setpts/asetpts` before selection.

**Step 2: Lock the seconds-based expression**

For an initial deletion and for multiple middle deletions, assert that the generated video expression uses `T-STARTT+first_keep_start` and subtracts each completed deletion duration before division by `TB`.

**Step 3: Run the focused test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml infrastructure::filter_script::tests`

Expected: FAIL because v1.1.0 normalizes PTS before selection and does not use `settb/asettb`.

### Task 2: Implement post-selection timebase normalization

**Files:**
- Modify: `src-tauri/src/infrastructure/filter_script.rs`

**Step 1: Remove pre-selection PTS rewriting**

Restore `select` and `asetnsamples/aselect` as the first timestamp-sensitive operations on their output branches.

**Step 2: Generate seconds-based compacted video PTS**

Build a source timeline term from `T-STARTT+<first keep start seconds>`. Subtract `gte(source_time,<delete end>)*<delete duration>` for every normalized deletion and divide the final seconds value by `TB`.

**Step 3: Set an explicit post-selection timebase**

Insert `settb=AVTB` before video `setpts`, and `asettb=AVTB` before the final audio `asetpts=N/SR/TB`.

**Step 4: Run focused tests**

Expected: filter graph tests pass.

### Task 3: Extend real FFmpeg regression coverage

**Files:**
- Modify: `src-tauri/src/infrastructure/ffmpeg_job.rs`

**Step 1: Retain the sparse-half VFR test**

Force the fixture video track to timebase `1/6000`, verify it with ffprobe, and continue deleting the initial five seconds so the first selected video frame is not source time zero.

**Step 2: Add multiple delete intervals**

Generate another VFR export plan with separated deletes across dense and sparse regions, then require existing duration, stream-layout, A/V drift and checkpoint validation to pass.

**Step 3: Run all explicit media tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml infrastructure::ffmpeg_job::ffmpeg_integration_tests:: -- --ignored --nocapture`

Expected: all explicit FFmpeg media tests pass.

### Task 4: Synchronize stable documentation

**Files:**
- Modify: `AGENTS.md`
- Modify: `docs/SpyCut-V1-开发文档.md`

**Step 1: Correct the export invariant**

Document that original-timeline selection precedes explicit post-selection timebase normalization.

**Step 2: Update the conceptual filter graph**

Replace pre-selection PTS rewriting with `select -> settb -> seconds-based setpts`, and restore the sample-count audio ordering.

### Task 5: Verify and inspect

**Files:**
- Inspect all changed files

**Step 1: Run the default gate**

Run: `pnpm check`

Expected: typecheck, frontend tests, Rust tests, Clippy and rustfmt pass.

**Step 2: Inspect repository state**

Run: `git diff --check`

Run: `git status --short`

Confirm no generated media and no SpyCut/FFmpeg/FFprobe processes remain. Record Windows true-hardware retest as outstanding until a new native package is exercised with the private source.
