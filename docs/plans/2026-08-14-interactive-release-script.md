# Interactive Release Script Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add one interactive shell command that safely bumps SpyCut's semantic version, verifies the repository, commits, tags and atomically pushes a GitHub Actions release.

**Architecture:** A macOS-compatible Bash script owns prompt and Git orchestration while Node performs targeted JSON/TOML version edits. The script validates the branch, remote, synchronized version files and tag state before mutation, restores version files on pre-publication failure, and uses an atomic branch-plus-tag push to avoid a half-published release.

**Tech Stack:** Bash 3.2+, Git, Node.js, pnpm, Cargo, Vitest, GitHub Actions

---

### Task 1: Define the script contract with tests

**Files:**
- Create: `src/release-workflow.test.ts`

**Step 1: Write failing version calculation tests**

Invoke `bash scripts/release.sh --calculate 1.2.3 patch|minor|major` and expect `1.2.4`, `1.3.0`, and `2.0.0`. Verify malformed versions and unknown increment types fail.

**Step 2: Write failing safety policy tests**

Read the script and require `pnpm check`, `git diff --check`, annotated tags, `git push --atomic`, all four version files, two confirmations, and no force push.

**Step 3: Run the focused test**

Run: `pnpm exec vitest run src/release-workflow.test.ts`

Expected: FAIL because `scripts/release.sh` does not exist.

### Task 2: Implement the release script

**Files:**
- Create: `scripts/release.sh`

**Step 1: Implement side-effect-free commands**

Add `--help` and `--calculate VERSION major|minor|patch` before repository checks so tests never access Git remotes or modify files.

**Step 2: Validate repository release state**

Require Git, Node and Cargo; resolve pnpm from a global installation, Corepack, or an npm fallback pinned to the repository version; enter the script-derived repository root; require branch `main`, remote `origin`, no conflict markers, synchronized package/Tauri/Cargo/lock versions, and a local branch that is not behind or diverged from `origin/main`.

**Step 3: Prompt and update versions**

Prompt for major/minor/patch, calculate the next version, reject existing tags, show `git status --short`, and confirm inclusion of all current changes. Back up and update `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, then run Cargo to update `src-tauri/Cargo.lock`.

**Step 4: Verify and publish**

Run `pnpm check` with `SPYCUT_RELEASE_IN_PROGRESS=1` so only the nested commit/tag/push integration simulation is skipped, then run `git diff --check`; show the final status/stat; require a second explicit confirmation; stage all reviewed changes, commit `release: vX.Y.Z`, create `git tag -a`, and atomically push branch and tag.

Set `GIT_PAGER=cat` for the complete script so no Git check or summary can open an interactive pager before the final confirmation.

**Step 5: Handle cancellation and retry**

Restore the four backed-up version files if checks fail or the final confirmation is declined. Detect an unpushed current-version tag at HEAD and offer an atomic retry only when the worktree is clean.

### Task 3: Document the new release entry point

**Files:**
- Modify: `README.md`
- Modify: `README.zh-CN.md`
- Modify: `AGENTS.md`

**Step 1: Add user-facing commands**

Document `scripts/release.sh`, its major/minor/patch prompt, its inclusion of all current worktree changes, and the fact that `v*` tags create prereleases while `workflow_dispatch` only uploads artifacts.

**Step 2: Update agent guidance**

Make the interactive script the preferred release entry point and retain native packaging and verification constraints.

### Task 4: Verify

**Files:**
- Test: `src/release-workflow.test.ts`

**Step 1: Check shell syntax**

Run: `bash -n scripts/release.sh`

Expected: PASS.

**Step 2: Run focused tests**

Run: `pnpm exec vitest run src/release-workflow.test.ts src/package-workflows.test.ts`

Expected: PASS without Git mutations.

**Step 3: Run repository gates**

Run: `pnpm check`

Run: `git diff --check`

Expected: PASS; no commits, tags or remote pushes were created by verification.
