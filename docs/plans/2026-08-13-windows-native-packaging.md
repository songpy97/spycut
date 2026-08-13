# Windows Native Packaging Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the macOS/Linux NSIS fallback with a Windows-native build that smoke-installs and uninstalls every publishable installer.

**Architecture:** macOS packaging produces only macOS artifacts. A PowerShell entry point builds NSIS on Windows, emits a companion SHA-256 file, and optionally performs an isolated install/uninstall smoke test; GitHub Actions runs that smoke test before uploading the Windows artifact.

**Tech Stack:** PowerShell 7, Tauri 2, pnpm, NSIS, GitHub Actions Windows 2022.

---

### Task 1: Windows-native packaging entry point

**Files:**
- Create: `scripts/package-windows.ps1`

1. Validate Windows, required tools and cleanly locate the repository.
2. Remove the previous installer, prepare pinned sidecars, run `pnpm check`, and build only the NSIS bundle.
3. Generate an adjacent lowercase SHA-256 file.
4. Under explicit `-SmokeTest`, refuse to touch a machine with an existing SpyCut uninstall entry, silently install to an isolated path, verify packaged files, silently uninstall, and confirm cleanup.

### Task 2: Stop cross-packaging Windows on macOS

**Files:**
- Modify: `scripts/package-all-macos.sh` (retired command that fails closed)
- Create: `scripts/package-macos.sh`

1. Retain the bounded macOS tests, app signing, ZIP and DMG verification.
2. Remove cargo-xwin, OrbStack, Docker and NSIS requirements and all Windows output handling.
3. Ensure the checksum manifest only lists artifacts actually produced and verified by this script.

### Task 3: Make Windows CI the release authority

**Files:**
- Modify: `.github/workflows/ci.yml`

1. Keep the existing macOS packaging path.
2. Route the Windows package job through `scripts/package-windows.ps1 -SkipChecks -SmokeTest`.
3. Upload only the smoke-tested setup EXE and companion SHA-256 file.

### Task 4: Align release documentation and repository guidance

**Files:**
- Modify: `README.md`
- Modify: `AGENTS.md`
- Modify: `docs/test-reports/windows-v1.md`
- Modify: `docs/release/SpyCut_0.1.0_checksums.txt`

1. Withdraw the cross-built NSIS installer and document Windows-native commands.
2. State that macOS cross compilation is compile-only and cannot create a publishable installer.
3. Record the failed cross-built artifact and require install/uninstall smoke validation for future checksums.

### Task 5: Verify

1. Parse the PowerShell script when `pwsh` is available and run shell syntax checks for the macOS script.
2. Run `pnpm check` and `git diff --check`.
3. Inspect workflow YAML and changed docs for stale Linux NSIS claims.
4. Stop OrbStack if this task started it and confirm no packaging containers or sessions remain.
