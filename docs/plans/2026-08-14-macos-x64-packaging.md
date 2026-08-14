# macOS x64 Packaging Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add native Intel macOS DMG and ZIP artifacts to the existing GitHub Actions release workflow.

**Architecture:** Reuse the native macOS packaging script on both GitHub-hosted architectures. Detect the host architecture inside the script, select the matching Tauri sidecar target, and give every uploaded package and checksum an architecture-specific name.

**Tech Stack:** GitHub Actions YAML, Bash, Tauri 2, Vitest

---

### Task 1: Add packaging policy coverage

**Files:**
- Modify: `src/package-workflows.test.ts`

**Step 1:** Add assertions for `macos-15-intel`, the Intel artifact name, both architecture mappings, and architecture-specific checksum names.

**Step 2:** Run `pnpm vitest run src/package-workflows.test.ts` and confirm the new assertions fail against the current workflow and script.

### Task 2: Generalize native macOS packaging

**Files:**
- Modify: `scripts/package-macos.sh`
- Modify: `.github/workflows/ci.yml`

**Step 1:** Map `uname -m` to the supported Tauri target and release filename suffix.

**Step 2:** Include the architecture suffix in DMG, ZIP, checksum, and Actions artifact names.

**Step 3:** Add the `macos-15-intel` package matrix entry and keep Release publication dependent on the whole package matrix.

**Step 4:** Run the focused Vitest file and `bash -n scripts/package-macos.sh`.

### Task 3: Synchronize release documentation

**Files:**
- Modify: `README.md`
- Modify: `README.zh-CN.md`
- Modify: `AGENTS.md`

**Step 1:** Document Intel macOS availability and architecture-specific checksum files.

**Step 2:** Update the repository workflow contract from two native package targets to three.

**Step 3:** Run `git diff --check`, inspect the scoped diff, then run `pnpm check`.
