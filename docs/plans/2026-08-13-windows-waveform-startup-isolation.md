# Windows Waveform Startup Isolation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Prevent Windows import-time crashes by delaying waveform IPC until the preview has loaded and the user explicitly requests analysis.

**Architecture:** `App.svelte` owns a Windows-only manual waveform state and starts analysis only after a `PlayerPane` loaded event. `TimelineEditor` exposes a small request control in the waveform lane. Existing Rust extraction remains unchanged, while the diagnostics command accepts one additional fixed lifecycle event for precise privacy-safe breadcrumbs.

**Tech Stack:** Svelte, TypeScript, Vitest, Tauri 2, Rust, FFmpeg sidecar, GitHub Actions.

---

### Task 1: Add failing frontend regression tests

**Files:**
- Modify: `src/App.test.ts`
- Modify: `src/lib/components/TimelineEditor.test.ts`

1. Test that Windows project import does not automatically call `getAudioWaveform`.
2. Test that preview-loaded enables manual generation and a click calls the command once.
3. Test that non-Windows behavior remains automatic.
4. Run focused Vitest and confirm the new assertions fail before implementation.

### Task 2: Isolate Windows waveform startup

**Files:**
- Modify: `src/App.svelte`
- Modify: `src/lib/components/TimelineEditor.svelte`
- Modify: `src/app.css`

1. Reset preview readiness whenever the project changes.
2. On Windows, enter a manual waiting state instead of invoking waveform analysis.
3. Mark preview ready from `PlayerPane`'s existing loaded event.
4. Emit a request event from the waveform lane and invoke analysis only after the user clicks.
5. Keep automatic loading on non-Windows platforms and preserve stale-result protection.

### Task 3: Add lifecycle diagnostics

**Files:**
- Modify: `src/lib/api/tauri.ts`
- Modify: `src/App.svelte`
- Modify: `src-tauri/src/commands/diagnostics.rs`

1. Add `waveform_lifecycle` to the fixed frontend diagnostic event union and backend allowlist.
2. Record privacy-safe session, preview and request stages.
3. Add or update tests for the allowlist and frontend calls.

### Task 4: Version, documentation and verification

**Files:**
- Modify: `package.json`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/Cargo.lock`
- Modify: `README.md`
- Modify: `AGENTS.md`
- Modify: `docs/test-reports/windows-v1.md`

1. Set the application version to 0.1.3 and refresh the Rust lockfile mechanically.
2. Document Windows manual waveform generation and the new lifecycle log stages.
3. Run focused tests, `pnpm check`, the real FFmpeg waveform test, Windows target Clippy, production build and diff checks.

### Task 5: Publish and monitor 0.1.3

1. Commit and push `main`.
2. Create and push annotated tag `v0.1.3`.
3. Monitor the native macOS and Windows workflow through package and Windows install/uninstall smoke verification.
4. Confirm all five release assets and checksums before delivery.
