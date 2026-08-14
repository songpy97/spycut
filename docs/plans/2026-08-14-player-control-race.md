# Player Control Race Fix Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Prevent normal pause, playback and timeline scrub operations from surfacing canceled playback promises or stale seek timeouts as user-facing failures.

**Architecture:** Keep browser media concurrency inside `HtmlVideoAdapter`: playback requests are invalidated by pause/load/dispose, and exact seeks use latest-wins completion with a finite timeout. Propagate the seek completion result through `PlayerPane` so `App.svelte` resumes playback only after the active scrub seek succeeds.

**Tech Stack:** Svelte 5, TypeScript 6, Vitest 4, HTMLMediaElement, Tauri 2 WebView

---

### Task 1: Lock the adapter behavior with regression tests

**Files:**
- Modify: `src/lib/player/HtmlVideoAdapter.test.ts`

**Step 1: Write failing playback tests**

Add tests that start a pending `play()`, call `pause()`, reject the browser Promise with `AbortError`, and expect the adapter Promise to resolve. Add a companion test proving a current `NotAllowedError` still rejects.

**Step 2: Write failing seek tests**

Add tests proving a newer exact seek returns `false` for the old request, a preview seek supersedes an exact seek, a missing `seeked` resolves only when `video.seeking` has become false at the target, and an active seek rejects after the finite timeout.

**Step 3: Run the focused test and verify failure**

Run: `pnpm exec vitest run src/lib/player/HtmlVideoAdapter.test.ts`

Expected: FAIL because canceled playback is still rejected and `seekTo()` does not return latest-wins completion.

### Task 2: Implement latest-wins media commands

**Files:**
- Modify: `src/lib/player/MediaPlayerAdapter.ts`
- Modify: `src/lib/player/HtmlVideoAdapter.ts`

**Step 1: Implement playback invalidation**

Track a playback request sequence. Increment it on pause, load and dispose; suppress only an expected `AbortError` from a request invalidated by a later action.

**Step 2: Implement seek completion results**

Change `seekTo(seconds)` to `Promise<boolean>`. Keep one pending exact seek, resolve it with `false` when a later preview or exact seek supersedes it, clamp targets to the element duration, and clean up all listeners and timers exactly once.

**Step 3: Keep the timeout finite**

At timeout, resolve `true` only when the active request is at the target and `video.seeking` is false; otherwise reject with `视频定位超时`.

**Step 4: Run the adapter tests**

Run: `pnpm exec vitest run src/lib/player/HtmlVideoAdapter.test.ts`

Expected: PASS.

### Task 3: Propagate completion through the player component

**Files:**
- Modify: `src/lib/components/PlayerPane.svelte`
- Modify: `src/lib/components/PlayerPane.test.ts`

**Step 1: Write the failing component test**

Mock an adapter seek returning `false`; assert the component returns `false` and does not dispatch a synthetic time update. Assert a `true` result is returned and dispatched.

**Step 2: Implement boolean propagation**

Return the adapter result from `PlayerPane.seekTo()`, dispatch time only for a completed current seek, and return `true` for demo positioning.

**Step 3: Run component tests**

Run: `pnpm exec vitest run src/lib/components/PlayerPane.test.ts`

Expected: PASS.

### Task 4: Resume playback only after a successful scrub seek

**Files:**
- Modify: `src/App.svelte`

**Step 1: Consume the seek completion result**

Require `true` before updating deletion guards, saving a scrub position, or considering a general seek current.

**Step 2: Gate scrub playback restoration**

Track whether commit/cancel positioning completed. Restore playback only when it did; leave the player paused after a timeout or superseded request.

**Step 3: Run app and timeline tests**

Run: `pnpm exec vitest run src/App.test.ts src/lib/components/TimelineEditor.test.ts`

Expected: PASS.

### Task 5: Update repository guidance and verify

**Files:**
- Modify: `AGENTS.md`

**Step 1: Record the stable preview concurrency rule**

Document that canceled playback is not a user-facing failure, exact seeks are latest-wins, and failed/superseded scrub seeks must not auto-resume.

**Step 2: Run frontend verification**

Run: `pnpm typecheck`

Run: `pnpm test`

Expected: all commands pass.

**Step 3: Inspect the final diff**

Run: `git diff --check`

Run: `git diff --name-status`

Expected: no whitespace errors; only intended player, tests, plans and guidance are added alongside pre-existing user changes.
