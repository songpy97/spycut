# Audio Waveform Track Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add an automatically generated, viewport-synchronized speech waveform between SpyCut's ruler and deletion interval lane.

**Architecture:** A new Rust infrastructure module streams mono 8 kHz PCM from a supervised FFmpeg child and reduces it to 50 peak samples per second. A project-scoped Tauri command returns the peaks; Svelte owns the async lifecycle and `TimelineEditor` renders only the visible, pixel-aggregated data as one SVG path.

**Tech Stack:** Rust, Tokio, FFmpeg sidecar, Tauri 2 IPC, Svelte 5, TypeScript, Canvas, Vitest.

---

### Task 1: Peak extraction and command boundary

**Files:**
- Create: `src-tauri/src/infrastructure/audio_waveform.rs`
- Modify: `src-tauri/src/infrastructure/mod.rs`
- Modify: `src-tauri/src/commands/project.rs`
- Modify: `src-tauri/src/lib.rs`

1. Add failing unit tests for split PCM reads, silence, loud samples and a partial final 20 ms bucket.
2. Run `cargo test --manifest-path src-tauri/Cargo.toml audio_waveform` and verify failure.
3. Implement streaming peak aggregation and finite-time FFmpeg supervision.
4. Add `get_audio_waveform(projectId)` with current-project validation before and after extraction.
5. Re-run the focused Rust tests and verify they pass.

### Task 2: Frontend lifecycle and waveform track

**Files:**
- Modify: `src/lib/types/contracts.ts`
- Modify: `src/lib/api/tauri.ts`
- Modify: `src/App.svelte`
- Modify: `src/lib/components/TimelineEditor.svelte`
- Modify: `src/app.css`

1. Add the `AudioWaveform` contract and API wrapper.
2. Add project-scoped loading, ready, unavailable and failed states in `App.svelte`; generate deterministic demo peaks.
3. Add an SVG waveform lane to `TimelineEditor`, aggregate visible peaks to the track width, and rebuild one path on viewport/size/data changes.
4. Keep ruler, waveform, deletion lane, playhead and navigator on the existing shared viewport and pointer interaction path.
5. Adjust the fixed timeline row height without shrinking the minimum video workspace below its existing constraint.

### Task 3: Regression coverage and documentation

**Files:**
- Modify: `src/lib/components/TimelineEditor.test.ts`
- Modify: `src/App.test.ts`
- Modify: `AGENTS.md`

1. Test waveform states and waveform-lane scrubbing.
2. Test automatic request and stale-result protection at the App boundary.
3. Document the new command/data flow and minimum verification in `AGENTS.md`.
4. Run `pnpm check` with a finite command timeout, then `git diff --check`.
5. Inspect the demo UI and fix any visible alignment or overflow regression before delivery.
