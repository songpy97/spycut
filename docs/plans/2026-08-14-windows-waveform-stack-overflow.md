# Windows Waveform Stack Overflow Fix Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Prevent the Windows release build from exiting when a user invokes audio waveform generation.

**Architecture:** Keep the existing FFmpeg streaming pipeline and IPC contract, but move the 64 KiB read buffer off the async state-machine stack and box the extraction future at the Tauri command boundary. Add an entry breadcrumb and a stable future-size regression test so this failure mode remains observable and cannot silently return.

**Tech Stack:** Rust 1.97.1, Tokio, Tauri 2, FFmpeg, Cargo tests

---

### Task 1: Add the future-size regression test

**Files:**
- Modify: `src-tauri/src/infrastructure/audio_waveform.rs`

**Step 1: Write the failing test**

Add a unit test that constructs `extract_audio_waveform(Path::new("ffmpeg"), Path::new("source.mp4"), 1_000_000)` without polling it, measures it with `std::mem::size_of_val`, and requires a size no greater than 4 KiB.

**Step 2: Run the focused test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml waveform_extraction_future_stays_small`

Expected: FAIL because the current future is approximately 66 KiB.

### Task 2: Move the buffer to the heap and box the command future

**Files:**
- Modify: `src-tauri/src/infrastructure/audio_waveform.rs:90-99`
- Modify: `src-tauri/src/commands/project.rs:69-104`

**Step 1: Implement the minimal extraction change**

Replace the fixed `[u8; 64 * 1024]` buffer with `vec![0_u8; 64 * 1024]`. Keep the read loop and `aggregator.push_bytes(&buffer[..count])` unchanged.

**Step 2: Make the command boundary explicit**

Record `waveform_command_entered stage=session_validation` before reading session state, then await `Box::pin(extract_audio_waveform(...))`.

**Step 3: Run focused tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml audio_waveform`

Expected: all non-ignored waveform tests pass, including the future-size regression.

**Step 4: Run the real FFmpeg test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml extracts_peaks_from_a_real_mp4_audio_stream -- --ignored`

Expected: PASS when FFmpeg is available.

### Task 3: Verify and document the Windows gap

**Files:**
- Modify: `docs/test-reports/windows-v1.md`
- Review: `AGENTS.md`

**Step 1: Record the evidence and fix boundary**

Document the two 0.1.4 reproductions, measured future sizes, implementation change, completed local checks, and the outstanding Windows native retest. Do not claim Windows success before a real-machine run.

**Step 2: Run the full repository checks**

Run: `pnpm check`

Expected: typecheck, frontend tests, Rust tests, Clippy, and rustfmt all pass.

**Step 3: Run whitespace and workspace checks**

Run: `git diff --check` and `git status --short`.

Expected: no whitespace errors and only intended source/documentation changes.

**Step 4: Commit only if requested**

Keep the verified changes in the working tree unless the user explicitly requests a commit.
