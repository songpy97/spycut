# VFR Export Timestamp Repair Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Preserve edit-plan wall-clock duration when exporting VFR MP4 sources to CFR output.

**Architecture:** Normalize input PTS, keep the existing original-timeline selection expression, compact video PTS by the cumulative duration of completed delete intervals, then run the `fps` filter at the planned output rate. Keep sample-count-based audio timestamp reconstruction and the unedited progress branch.

**Tech Stack:** Rust, Tauri 2, FFmpeg/FFprobe filter graphs, Cargo tests

---

### Task 1: Lock the filter graph contract

**Files:**
- Modify: `src-tauri/src/infrastructure/filter_script.rs`

**Step 1: Write failing unit assertions**

Require the generated graph to normalize video PTS before selection, subtract each completed delete interval duration from the original PTS, apply an explicit `fps` filter, and omit `setpts=N/(fps*TB)`.

**Step 2: Run the focused unit test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml infrastructure::filter_script::tests`

Expected: FAIL because the current graph reconstructs video time from the selected frame count.

### Task 2: Add a real VFR regression

**Files:**
- Modify: `src-tauri/src/infrastructure/ffmpeg_job.rs`

**Step 1: Generate a deterministic VFR fixture**

Create a ten-second H.264/AAC MP4 whose first five seconds retain every 30 fps frame and whose last five seconds retain one of every three frames while preserving source PTS.

**Step 2: Export only the sparse half**

Build an `ExportPlan` deleting `[0,5s)`, run the production filter and command construction, and call `validate_export`.

**Step 3: Run the ignored regression explicitly**

Run: `cargo test --manifest-path src-tauri/Cargo.toml infrastructure::ffmpeg_job::ffmpeg_integration_tests::exact_export_preserves_vfr_wall_clock_duration -- --ignored --exact`

Expected before implementation: FAIL with a duration mismatch of roughly 2.5 seconds.

### Task 3: Preserve wall-clock PTS before CFR conversion

**Files:**
- Modify: `src-tauri/src/infrastructure/filter_script.rs`

**Step 1: Build the cumulative delete-offset expression**

For each normalized `[start,end)` deletion, emit `gte(PTS*TB,end_seconds)*duration_seconds` and join the terms with `+`.

**Step 2: Change the video filter chain**

Emit `setpts=PTS-STARTPTS` before `select`, compact selected PTS by the cumulative delete offset, and apply `fps=<num>/<den>:start_time=0` before pixel-format conversion.

**Step 3: Preserve audio and progress semantics**

Normalize audio PTS before `asetnsamples` and `aselect`, retain `asetpts=N/SR/TB`, and leave the unedited scan branch tied to the original source timeline.

**Step 4: Run focused unit and real FFmpeg tests**

Expected: filter unit tests and the VFR regression pass.

### Task 4: Synchronize stable documentation

**Files:**
- Modify: `docs/SpyCut-V1-开发文档.md`
- Modify: `AGENTS.md`

**Step 1: Update the conceptual filter graph**

Document that video PTS removes cumulative deleted wall-clock time before CFR conversion and that audio remains sample-count based.

**Step 2: Record the export invariant**

Require VFR export to preserve retained wall-clock duration rather than deriving duration from selected frame count.

### Task 5: Verify the full repository

**Files:**
- Inspect all changed files

**Step 1: Run Rust formatting and focused tests**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`

Run the filter-script unit tests and explicit real-FFmpeg VFR regression.

**Step 2: Run the default repository gate**

Run: `pnpm check`

Expected: all non-platform-specific checks pass.

**Step 3: Inspect final state**

Run: `git diff --check`

Run: `git status --short`

Expected: no whitespace errors, no generated media or process/session leftovers, and only intended source, test, documentation, and plan changes.
