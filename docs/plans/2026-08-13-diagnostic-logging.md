# Diagnostic Logging Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Persist privacy-safe crash breadcrumbs and uncaught errors, expose the log to users, and suppress Windows media-process console windows.

**Architecture:** A synchronous, size-bounded Rust logger writes fixed diagnostic events and maintains an unclean-session marker. Tauri commands accept sanitized frontend failures and expose the controlled log path; all production FFmpeg/FFprobe commands share a Windows-aware constructor.

**Tech Stack:** Rust standard library, Tokio, Tauri 2, Svelte, TypeScript, Vitest, cargo-xwin.

---

### Task 1: Persistent diagnostic log

**Files:**
- Create: `src-tauri/src/infrastructure/diagnostics.rs`
- Modify: `src-tauri/src/infrastructure/mod.rs`

1. Add failing tests for line flushing, 5 MiB rotation, unclean marker detection, clean shutdown and path/URL redaction.
2. Run the focused Rust tests and verify they fail.
3. Implement `DiagnosticLog`, synchronous records, rotation, marker lifecycle and panic hook.
4. Re-run the focused tests and verify they pass.

### Task 2: Tauri and media-process integration

**Files:**
- Create: `src-tauri/src/commands/diagnostics.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/infrastructure/tool_locator.rs`
- Modify: production FFmpeg/FFprobe call sites under `src-tauri/src/`

1. Register the logger during setup and record startup/clean-exit events.
2. Add `get_diagnostic_status` and validated `record_frontend_diagnostic` commands.
3. Instrument source opening and waveform analysis with privacy-safe breadcrumbs.
4. Add a shared media command constructor that applies `CREATE_NO_WINDOW` on Windows and use it for every production media subprocess.
5. Run native Rust tests and Windows-target Clippy/link checks.

### Task 3: Frontend reporting and log access

**Files:**
- Modify: `src/lib/types/contracts.ts`
- Modify: `src/lib/api/tauri.ts`
- Modify: `src/App.svelte`
- Modify: `src/App.test.ts`
- Modify: `src/app.css`

1. Add failing tests for automatic global-error reporting and the log reveal action.
2. Add frontend API wrappers and install `error`/`unhandledrejection` listeners during mount.
3. Add a small “诊断日志” header action that reveals the controlled log file.
4. Re-run focused tests and visually verify the header at normal and narrow widths.

### Task 4: Documentation and full verification

**Files:**
- Modify: `AGENTS.md`
- Modify: `README.md`
- Modify: `docs/test-reports/windows-v1.md`

1. Document the diagnostics path, privacy policy, marker semantics and Windows black-window fix.
2. Run `pnpm check`, the ignored real-FFmpeg waveform test, Windows target checks when available, and `git diff --check`.
3. Confirm no test logs, app processes, FFmpeg processes, terminal sessions or local servers remain.
