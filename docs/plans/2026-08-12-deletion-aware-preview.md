# Deletion-Aware Preview Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make ordinary preview playback skip saved deletion intervals while allowing a manually selected deletion interval to play normally once.

**Architecture:** Add a pure `DeletionPlaybackGuard` under the player library to distinguish continuous playback from explicit navigation. Integrate it in `App.svelte`, keeping the existing join-review jump as the higher-priority playback mode.

**Tech Stack:** Svelte 5, TypeScript, Vitest

---

### Task 1: Deletion playback state machine

**Files:**
- Create: `src/lib/player/DeletionPlaybackGuard.ts`
- Create: `src/lib/player/DeletionPlaybackGuard.test.ts`

**Step 1: Write the failing tests**

Add tests proving that continuous playback returns the first crossed deletion interval, a manual position inside an interval exempts only that interval, an update can detect a fully crossed short interval, and paused observations never request a skip.

**Step 2: Run the focused test to verify it fails**

Run: `pnpm test -- src/lib/player/DeletionPlaybackGuard.test.ts`

Expected: FAIL because `DeletionPlaybackGuard.ts` does not exist.

**Step 3: Implement the minimal state machine**

Implement `setManualPosition`, `setAutomaticPosition`, `observePlayback`, and `reset`. Treat intervals as `[startUs, endUs)` and compare by stable interval ID for the temporary exemption.

**Step 4: Run the focused test to verify it passes**

Run: `pnpm test -- src/lib/player/DeletionPlaybackGuard.test.ts`

Expected: PASS.

### Task 2: Application playback integration

**Files:**
- Modify: `src/App.svelte`
- Test: `src/lib/player/DeletionPlaybackGuard.test.ts`

**Step 1: Register explicit navigation**

Update the shared seek helper and scrub commit/cancel paths to call `setManualPosition`. Use `setAutomaticPosition` for deletion skips and the existing join-review jump.

**Step 2: Apply the guard to playback updates**

After the existing review-jump branch, inspect ordinary playing updates. When the guard returns an interval, seek once to `endUs`, preserve playback, and suppress duplicate time callbacks until the seek settles.

**Step 3: Handle failed automatic seeks**

Pause playback and show a contextual error if the automatic seek fails. Do not alter intervals or persisted project data.

**Step 4: Run frontend verification**

Run: `pnpm typecheck`

Expected: no Svelte or TypeScript errors.

Run: `pnpm test`

Expected: all Vitest tests pass.

### Task 3: Repository verification

**Files:**
- Review: `AGENTS.md`
- Review: all changed files

**Step 1: Check whether repository guidance needs synchronization**

Compare `git diff --name-status` with AGENTS.md sections 2–7. This change does not add IPC or alter persistence/export invariants; update AGENTS.md only if the implemented responsibilities differ from its current player description.

**Step 2: Run whitespace validation**

Run: `git diff --check`

Expected: no whitespace errors.

**Step 3: Review final scope**

Run: `git status --short` and a targeted `git diff` for the implementation files. Confirm existing user changes remain untouched and no generated files were added.
