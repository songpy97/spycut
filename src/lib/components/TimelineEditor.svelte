<script lang="ts">
  import { createEventDispatcher, onDestroy, onMount } from "svelte";
  import type { AudioWaveform, DeleteInterval } from "../types/contracts";
  import { formatTime } from "../utils/time";
  import {
    buildTimelineTicks,
    clampViewport,
    panViewport,
    sliderFromViewport,
    snapToFrame,
    spanFromSlider,
    timeAtClientX,
    zoomAtAnchor,
    zoomToSpan,
    type TimelineBounds,
    type TimelineViewport
  } from "../timeline/viewport";
  import TimelineToolbar from "./TimelineToolbar.svelte";

  export let durationUs: number;
  export let frameDurationUs: number;
  export let playheadUs: number;
  export let intervals: DeleteInterval[] = [];
  export let selectedId: number | null = null;
  export let pendingStartUs: number | null = null;
  export let canUndo = false;
  export let canRedo = false;
  export let playing = false;
  export let editLocked = false;
  export let markSaving = false;
  export let waveform: AudioWaveform | null = null;
  export let waveformState: "loading" | "ready" | "unavailable" | "failed" = "unavailable";
  export let waveformMessage = "";

  const dispatch = createEventDispatcher<{
    scrubStart: { playheadUs: number };
    scrubPreview: { playheadUs: number };
    scrubCommit: { playheadUs: number };
    scrubCancel: void;
    select: { id: number; playheadUs: number };
    resize: { id: number; startUs: number; endUs: number };
    markStart: void;
    markEnd: void;
    cancelMark: void;
    undo: void;
    redo: void;
  }>();

  type ScrubGesture = { kind: "scrub"; pointerId: number; originUs: number; currentUs: number };
  type ResizeGesture = {
    kind: "resize";
    pointerId: number;
    intervalId: number;
    edge: "start" | "end";
    originalStartUs: number;
    originalEndUs: number;
    previewStartUs: number;
    previewEndUs: number;
  };
  type NavigateGesture = { kind: "navigate"; pointerId: number; grabOffsetRatio: number };
  type TimelineGesture = { kind: "idle" } | ScrubGesture | ResizeGesture | NavigateGesture;
  type VisibleInterval = {
    interval: DeleteInterval;
    shown: DeleteInterval;
    leftPercent: number;
    widthPercent: number;
  };

  let editor: HTMLElement;
  let track: HTMLDivElement;
  let navigator: HTMLDivElement;
  let trackWidth = 1_000;
  let hoverUs: number | null = null;
  let gesture: TimelineGesture = { kind: "idle" };
  let captureTarget: HTMLElement | null = null;
  let viewport: TimelineViewport = { startUs: 0, spanUs: 0 };
  let viewportDurationUs = -1;
  let lastFollowedPlayheadUs = -1;
  let lastPointerClientX: number | null = null;
  let autoPanFrame: number | null = null;
  let autoPanPreviousTime = 0;

  $: bounds = { durationUs, frameDurationUs: Math.max(1, frameDurationUs) } satisfies TimelineBounds;
  $: if (durationUs !== viewportDurationUs) initializeViewport();
  $: if (gesture.kind === "idle" && playheadUs !== lastFollowedPlayheadUs) followExternalPlayhead();
  $: visibleIntervals = buildVisibleIntervals(intervals, gesture, viewport);
  $: ticks = buildTimelineTicks(viewport, trackWidth, bounds);
  $: zoomValue = sliderFromViewport(viewport, bounds);
  $: navigatorLeft = durationUs > 0 ? (viewport.startUs / durationUs) * 100 : 0;
  $: navigatorWidth = durationUs > 0 ? (viewport.spanUs / durationUs) * 100 : 100;
  $: waveformPath = buildWaveformPath(waveform, waveformState, viewport, trackWidth);

  function initializeViewport() {
    viewportDurationUs = durationUs;
    if (durationUs <= 0) {
      viewport = { startUs: 0, spanUs: 0 };
      return;
    }
    const spanUs = Math.min(durationUs, 300_000_000);
    viewport = clampViewport({ startUs: playheadUs - spanUs / 2, spanUs }, bounds);
    lastFollowedPlayheadUs = playheadUs;
  }

  function followExternalPlayhead() {
    lastFollowedPlayheadUs = playheadUs;
    if (viewport.spanUs <= 0 || durationUs <= 0) return;
    const leftThreshold = viewport.startUs;
    const rightThreshold = viewport.startUs + viewport.spanUs * 0.85;
    if (playheadUs < leftThreshold || playheadUs > viewport.startUs + viewport.spanUs) {
      viewport = clampViewport({ ...viewport, startUs: playheadUs - viewport.spanUs / 2 }, bounds);
    } else if (playing && playheadUs >= rightThreshold) {
      viewport = panViewport(viewport, viewport.spanUs * 0.7, bounds);
    }
  }

  function percentInViewport(timeUs: number, currentViewport: TimelineViewport): number {
    if (currentViewport.spanUs <= 0) return 0;
    return ((timeUs - currentViewport.startUs) / currentViewport.spanUs) * 100;
  }

  function globalPercent(timeUs: number): number {
    return durationUs > 0 ? Math.min(100, Math.max(0, (timeUs / durationUs) * 100)) : 0;
  }

  function buildWaveformPath(
    source: AudioWaveform | null,
    state: typeof waveformState,
    currentViewport: TimelineViewport,
    width: number
  ): string {
    if (!source || state !== "ready" || source.peaks.length === 0 || source.samplesPerSecond <= 0 || currentViewport.spanUs <= 0) return "";
    const columns = Math.max(1, Math.round(width));
    const firstSample = Math.max(0, Math.floor((currentViewport.startUs / 1_000_000) * source.samplesPerSecond));
    const lastSample = Math.min(
      source.peaks.length,
      Math.ceil(((currentViewport.startUs + currentViewport.spanUs) / 1_000_000) * source.samplesPerSecond)
    );
    const visibleSamples = Math.max(1, lastSample - firstSample);
    const commands: string[] = [];
    for (let column = 0; column < columns; column += 1) {
      const sampleStart = Math.min(lastSample, firstSample + Math.floor((column / columns) * visibleSamples));
      const sampleEnd = Math.min(lastSample, Math.max(sampleStart + 1, firstSample + Math.ceil(((column + 1) / columns) * visibleSamples)));
      let peak = 0;
      for (let index = sampleStart; index < sampleEnd; index += 1) peak = Math.max(peak, source.peaks[index] ?? 0);
      if (peak === 0) continue;
      const amplitude = Math.max(1, (peak / 255) * 46);
      const x = column + .5;
      commands.push(`M${x} ${50 - amplitude}V${50 + amplitude}`);
    }
    return commands.join("");
  }

  function snappedTimeAt(clientX: number): number {
    if (!track) return snapToFrame(playheadUs, bounds);
    const rect = track.getBoundingClientRect();
    return snapToFrame(timeAtClientX(clientX, rect.left, rect.width, viewport), bounds);
  }

  function capturePointer(event: PointerEvent) {
    captureTarget = event.currentTarget as HTMLElement;
    captureTarget.setPointerCapture?.(event.pointerId);
  }

  function releasePointer(pointerId: number) {
    if (captureTarget?.hasPointerCapture?.(pointerId)) captureTarget.releasePointerCapture(pointerId);
    captureTarget = null;
  }

  function beginScrub(event: PointerEvent, intervalId?: number) {
    if (event.button !== 0 || durationUs <= 0 || gesture.kind !== "idle") return;
    event.preventDefault();
    event.stopPropagation();
    const nextUs = snappedTimeAt(event.clientX);
    if (intervalId !== undefined) dispatch("select", { id: intervalId, playheadUs: nextUs });
    capturePointer(event);
    gesture = { kind: "scrub", pointerId: event.pointerId, originUs: playheadUs, currentUs: nextUs };
    lastPointerClientX = event.clientX;
    dispatch("scrubStart", { playheadUs });
    dispatch("scrubPreview", { playheadUs: nextUs });
    updateAutoPan();
  }

  function beginResize(event: PointerEvent, interval: DeleteInterval, edge: "start" | "end") {
    if (event.button !== 0 || editLocked || gesture.kind !== "idle") return;
    event.preventDefault();
    event.stopPropagation();
    capturePointer(event);
    gesture = {
      kind: "resize",
      pointerId: event.pointerId,
      intervalId: interval.id,
      edge,
      originalStartUs: interval.startUs,
      originalEndUs: interval.endUs,
      previewStartUs: interval.startUs,
      previewEndUs: interval.endUs
    };
    lastPointerClientX = event.clientX;
    updateResizeAt(event.clientX);
    updateAutoPan();
  }

  function updateResizeAt(clientX: number) {
    if (gesture.kind !== "resize") return;
    const nextUs = snappedTimeAt(clientX);
    if (gesture.edge === "start") {
      gesture = {
        ...gesture,
        previewStartUs: Math.max(0, Math.min(nextUs, gesture.previewEndUs - frameDurationUs))
      };
    } else {
      gesture = {
        ...gesture,
        previewEndUs: Math.min(durationUs, Math.max(nextUs, gesture.previewStartUs + frameDurationUs))
      };
    }
  }

  function beginNavigate(event: PointerEvent, fromWindow: boolean) {
    if (event.button !== 0 || durationUs <= 0 || gesture.kind !== "idle") return;
    event.preventDefault();
    event.stopPropagation();
    const rect = navigator.getBoundingClientRect();
    if (rect.width <= 0) return;
    const pointerRatio = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
    const windowRatio = viewport.spanUs / durationUs;
    const leftRatio = viewport.startUs / durationUs;
    const grabOffsetRatio = fromWindow ? Math.min(windowRatio, Math.max(0, pointerRatio - leftRatio)) : windowRatio / 2;
    if (!fromWindow) viewport = clampViewport({ ...viewport, startUs: pointerRatio * durationUs - viewport.spanUs / 2 }, bounds);
    capturePointer(event);
    gesture = { kind: "navigate", pointerId: event.pointerId, grabOffsetRatio };
  }

  function movePointer(event: PointerEvent) {
    if (gesture.kind === "idle") {
      if (track?.contains(event.target as Node)) hoverUs = snappedTimeAt(event.clientX);
      return;
    }
    if (event.pointerId !== gesture.pointerId) return;
    lastPointerClientX = event.clientX;
    if (gesture.kind === "scrub") {
      const nextUs = snappedTimeAt(event.clientX);
      gesture = { ...gesture, currentUs: nextUs };
      dispatch("scrubPreview", { playheadUs: nextUs });
      updateAutoPan();
    } else if (gesture.kind === "resize") {
      updateResizeAt(event.clientX);
      updateAutoPan();
    } else if (gesture.kind === "navigate") {
      const rect = navigator.getBoundingClientRect();
      if (rect.width <= 0) return;
      const pointerRatio = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
      viewport = clampViewport({ ...viewport, startUs: (pointerRatio - gesture.grabOffsetRatio) * durationUs }, bounds);
    }
  }

  function endPointer(event: PointerEvent) {
    if (gesture.kind === "idle" || event.pointerId !== gesture.pointerId) return;
    const completed = gesture;
    stopAutoPan();
    gesture = { kind: "idle" };
    releasePointer(event.pointerId);
    lastPointerClientX = null;
    if (completed.kind === "scrub") {
      lastFollowedPlayheadUs = completed.currentUs;
      dispatch("scrubCommit", { playheadUs: completed.currentUs });
    } else if (completed.kind === "resize") {
      dispatch("resize", {
        id: completed.intervalId,
        startUs: completed.previewStartUs,
        endUs: completed.previewEndUs
      });
    }
  }

  function cancelPointer(pointerId?: number) {
    if (gesture.kind === "idle" || (pointerId !== undefined && gesture.pointerId !== pointerId)) return;
    const cancelled = gesture;
    stopAutoPan();
    gesture = { kind: "idle" };
    releasePointer(cancelled.pointerId);
    lastPointerClientX = null;
    if (cancelled.kind === "scrub") dispatch("scrubCancel");
  }

  function buildVisibleIntervals(
    sourceIntervals: DeleteInterval[],
    currentGesture: TimelineGesture,
    currentViewport: TimelineViewport
  ): VisibleInterval[] {
    const viewportEndUs = currentViewport.startUs + currentViewport.spanUs;
    return sourceIntervals.flatMap((interval) => {
      const shown = currentGesture.kind === "resize" && currentGesture.intervalId === interval.id
        ? { id: interval.id, startUs: currentGesture.previewStartUs, endUs: currentGesture.previewEndUs }
        : interval;
      if (shown.endUs <= currentViewport.startUs || shown.startUs >= viewportEndUs) return [];
      const clippedStartUs = Math.max(currentViewport.startUs, shown.startUs);
      const clippedEndUs = Math.min(viewportEndUs, shown.endUs);
      const leftPercent = percentInViewport(clippedStartUs, currentViewport);
      return [{
        interval,
        shown,
        leftPercent,
        widthPercent: Math.max(0, percentInViewport(clippedEndUs, currentViewport) - leftPercent)
      }];
    });
  }

  function resizeWithKeyboard(event: KeyboardEvent, interval: DeleteInterval, edge: "start" | "end") {
    if (editLocked || (event.key !== "ArrowLeft" && event.key !== "ArrowRight")) return;
    event.preventDefault();
    event.stopPropagation();
    const amount = event.shiftKey ? 1_000_000 : frameDurationUs;
    const delta = event.key === "ArrowLeft" ? -amount : amount;
    if (edge === "start") {
      dispatch("resize", {
        id: interval.id,
        startUs: snapToFrame(Math.max(0, Math.min(interval.endUs - frameDurationUs, interval.startUs + delta)), bounds),
        endUs: interval.endUs
      });
    } else {
      dispatch("resize", {
        id: interval.id,
        startUs: interval.startUs,
        endUs: snapToFrame(Math.min(durationUs, Math.max(interval.startUs + frameDurationUs, interval.endUs + delta)), bounds)
      });
    }
  }

  function movePlayheadWithKeyboard(event: KeyboardEvent) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    event.stopPropagation();
    const amount = event.shiftKey ? 1_000_000 : frameDurationUs;
    const nextUs = snapToFrame(playheadUs + (event.key === "ArrowLeft" ? -amount : amount), bounds);
    dispatch("scrubStart", { playheadUs });
    dispatch("scrubCommit", { playheadUs: nextUs });
  }

  function navigateWithKeyboard(event: KeyboardEvent) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    event.stopPropagation();
    const delta = viewport.spanUs * (event.shiftKey ? 0.8 : 0.1) * (event.key === "ArrowLeft" ? -1 : 1);
    viewport = panViewport(viewport, delta, bounds);
  }

  function fitTimeline() {
    viewport = { startUs: 0, spanUs: Math.max(0, durationUs) };
  }

  function zoomStep(direction: number) {
    if (durationUs <= 0) return;
    const anchorUs = playheadUs >= viewport.startUs && playheadUs <= viewport.startUs + viewport.spanUs
      ? playheadUs
      : viewport.startUs + viewport.spanUs / 2;
    const spanUs = direction > 0 ? viewport.spanUs / 1.25 : viewport.spanUs * 1.25;
    viewport = zoomToSpan(viewport, spanUs, anchorUs, bounds);
  }

  function setZoomValue(value: number) {
    if (durationUs <= 0) return;
    const anchorUs = playheadUs >= viewport.startUs && playheadUs <= viewport.startUs + viewport.spanUs
      ? playheadUs
      : viewport.startUs + viewport.spanUs / 2;
    viewport = zoomToSpan(viewport, spanFromSlider(value, bounds), anchorUs, bounds);
  }

  function handleWheel(event: WheelEvent) {
    if (!track || durationUs <= 0) return;
    const target = event.target as Node;
    const ruler = editor.querySelector("[data-timeline-ruler]");
    if (!track.contains(target) && !navigator?.contains(target) && !ruler?.contains(target)) return;
    if (event.altKey) {
      event.preventDefault();
      const rect = track.getBoundingClientRect();
      const anchorRatio = rect.width > 0 ? Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width)) : 0.5;
      const delta = Math.abs(event.deltaY) >= Math.abs(event.deltaX) ? event.deltaY : event.deltaX;
      viewport = zoomAtAnchor(viewport, delta, anchorRatio, bounds);
    } else if (Math.abs(event.deltaX) > 0 || event.shiftKey) {
      event.preventDefault();
      const deltaPx = Math.abs(event.deltaX) > 0 ? event.deltaX : event.deltaY;
      const width = Math.max(1, track.getBoundingClientRect().width);
      viewport = panViewport(viewport, (deltaPx / width) * viewport.spanUs, bounds);
    }
  }

  function updateAutoPan() {
    if (gesture.kind !== "scrub" && gesture.kind !== "resize") return;
    if (autoPanFrame === null) {
      autoPanPreviousTime = performance.now();
      autoPanFrame = requestAnimationFrame(runAutoPan);
    }
  }

  function runAutoPan(now: number) {
    autoPanFrame = null;
    if ((gesture.kind !== "scrub" && gesture.kind !== "resize") || lastPointerClientX === null || !track) return;
    const rect = track.getBoundingClientRect();
    const edgeSize = 24;
    let strength = 0;
    if (lastPointerClientX < rect.left + edgeSize) strength = -Math.min(1, (rect.left + edgeSize - lastPointerClientX) / edgeSize);
    else if (lastPointerClientX > rect.right - edgeSize) strength = Math.min(1, (lastPointerClientX - (rect.right - edgeSize)) / edgeSize);
    if (strength !== 0) {
      const elapsedSeconds = Math.min(0.05, Math.max(0, (now - autoPanPreviousTime) / 1_000));
      const speed = viewport.spanUs * (0.02 + Math.abs(strength) * 0.38);
      viewport = panViewport(viewport, Math.sign(strength) * speed * elapsedSeconds, bounds);
      if (gesture.kind === "scrub") {
        const nextUs = snappedTimeAt(lastPointerClientX);
        gesture = { ...gesture, currentUs: nextUs };
        dispatch("scrubPreview", { playheadUs: nextUs });
      } else updateResizeAt(lastPointerClientX);
    }
    autoPanPreviousTime = now;
    autoPanFrame = requestAnimationFrame(runAutoPan);
  }

  function stopAutoPan() {
    if (autoPanFrame !== null) cancelAnimationFrame(autoPanFrame);
    autoPanFrame = null;
  }

  function handleWindowKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && gesture.kind !== "idle") {
      event.preventDefault();
      event.stopPropagation();
      cancelPointer();
    }
  }

  onMount(() => {
    editor.addEventListener("wheel", handleWheel, { passive: false });
    window.addEventListener("keydown", handleWindowKeydown, true);
  });

  onDestroy(() => {
    editor?.removeEventListener("wheel", handleWheel);
    window.removeEventListener("keydown", handleWindowKeydown, true);
    cancelPointer();
    stopAutoPan();
  });
</script>

<section
  bind:this={editor}
  class="timeline-editor"
  class:is-scrubbing={gesture.kind === "scrub"}
  class:edit-locked={editLocked}
  role="group"
  aria-label="源视频编辑时间轴"
  data-testid="timeline-editor"
  on:pointermove={movePointer}
  on:pointerup={endPointer}
  on:pointercancel={(event) => cancelPointer(event.pointerId)}
  on:lostpointercapture={(event) => cancelPointer(event.pointerId)}
  on:pointerleave={() => { if (gesture.kind === "idle") hoverUs = null; }}
>
  <TimelineToolbar
    {playheadUs}
    {durationUs}
    {frameDurationUs}
    {pendingStartUs}
    {canUndo}
    {canRedo}
    {markSaving}
    {editLocked}
    {zoomValue}
    on:markStart={() => dispatch("markStart")}
    on:markEnd={() => dispatch("markEnd")}
    on:cancelMark={() => dispatch("cancelMark")}
    on:undo={() => dispatch("undo")}
    on:redo={() => dispatch("redo")}
    on:fit={fitTimeline}
    on:zoomStep={(event) => zoomStep(event.detail.direction)}
    on:zoomValue={(event) => setZoomValue(event.detail.value)}
  />

  <div class="timeline-stage">
    <div class="timeline-ruler" data-timeline-ruler role="presentation" on:pointerdown={beginScrub}>
      {#each ticks as tick}
        <span class:major={tick.major} style={`left:${percentInViewport(tick.timeUs, viewport)}%`}>
          <i></i>{#if tick.label}<b>{tick.label}</b>{/if}
        </span>
      {/each}
    </div>

    <div
      bind:this={track}
      bind:clientWidth={trackWidth}
      id="spycut-timeline-track"
      class="timeline-track"
      data-testid="timeline-track"
      role="presentation"
      on:pointerdown={beginScrub}
    >
      <div class="timeline-grid" aria-hidden="true"></div>
      <div class="timeline-waveform" data-testid="waveform-track">
        {#if waveformState === "ready" && waveformPath}
          <svg viewBox={`0 0 ${Math.max(1, trackWidth)} 100`} preserveAspectRatio="none" role="img" aria-label="源音频波形">
            <line x1="0" y1="50" x2={Math.max(1, trackWidth)} y2="50"></line>
            <path d={waveformPath}></path>
          </svg>
          <span class="timeline-waveform-label">音频波形</span>
        {:else}
          <div class:loading={waveformState === "loading"} class:failed={waveformState === "failed"} class="timeline-waveform-status" role="status" aria-label="音频波形状态">
            {#if waveformState === "loading"}<i></i>{/if}
            <span>{waveformMessage || "没有可显示的音频波形"}</span>
          </div>
        {/if}
      </div>
      <div class="timeline-delete-label"><b>删除区间轨道</b><span>源时间轴锁定</span></div>

      {#each visibleIntervals as visible (visible.interval.id)}
        {@const interval = visible.interval}
        {@const shown = visible.shown}
        <div
          class="timeline-delete-wrap"
          style={`left:${visible.leftPercent}%;width:${visible.widthPercent}%`}
        >
          <button
            type="button"
            class="timeline-delete-region"
            class:selected={interval.id === selectedId}
            aria-label={`删除区间 ${interval.id}，${formatTime(shown.startUs, true)} 到 ${formatTime(shown.endUs, true)}`}
            aria-pressed={interval.id === selectedId}
            on:pointerdown={(event) => beginScrub(event, interval.id)}
          ><span>DELETE</span></button>
          {#if interval.id === selectedId && !editLocked}
            <span
              class="timeline-edge-handle start"
              role="slider"
              tabindex="0"
              aria-label="调整删除起点"
              aria-valuemin="0"
              aria-valuemax={shown.endUs - frameDurationUs}
              aria-valuenow={shown.startUs}
              aria-valuetext={formatTime(shown.startUs, true)}
              on:pointerdown={(event) => beginResize(event, interval, "start")}
              on:keydown={(event) => resizeWithKeyboard(event, interval, "start")}
            ></span>
            <span
              class="timeline-edge-handle end"
              role="slider"
              tabindex="0"
              aria-label="调整删除终点"
              aria-valuemin={shown.startUs + frameDurationUs}
              aria-valuemax={durationUs}
              aria-valuenow={shown.endUs}
              aria-valuetext={formatTime(shown.endUs, true)}
              on:pointerdown={(event) => beginResize(event, interval, "end")}
              on:keydown={(event) => resizeWithKeyboard(event, interval, "end")}
            ></span>
          {/if}
        </div>
      {/each}

      {#if pendingStartUs !== null}
        {@const pendingLeft = Math.max(viewport.startUs, Math.min(pendingStartUs, playheadUs))}
        {@const pendingRight = Math.min(viewport.startUs + viewport.spanUs, Math.max(pendingStartUs, playheadUs))}
        {#if pendingRight >= viewport.startUs && pendingLeft <= viewport.startUs + viewport.spanUs}
          <div
            class="timeline-pending-region"
            style={`left:${percentInViewport(pendingLeft, viewport)}%;width:${Math.max(0, percentInViewport(pendingRight, viewport) - percentInViewport(pendingLeft, viewport))}%`}
          ><span>待完成删除区间</span></div>
        {/if}
      {/if}

      {#if hoverUs !== null && gesture.kind === "idle" && Math.abs(hoverUs - playheadUs) > frameDurationUs}
        <div class="timeline-hover-head" class:near-right={percentInViewport(hoverUs, viewport) > 84} style={`left:${percentInViewport(hoverUs, viewport)}%`}><span>{formatTime(hoverUs, true)}</span></div>
      {/if}

      {#if playheadUs >= viewport.startUs && playheadUs <= viewport.startUs + viewport.spanUs}
        <div
          class="timeline-playhead"
          class:near-right={percentInViewport(playheadUs, viewport) > 84}
          style={`left:${percentInViewport(playheadUs, viewport)}%`}
          role="slider"
          tabindex="0"
          aria-label="当前播放位置"
          aria-valuemin="0"
          aria-valuemax={durationUs}
          aria-valuenow={playheadUs}
          aria-valuetext={formatTime(playheadUs, true)}
          on:pointerdown={beginScrub}
          on:keydown={movePlayheadWithKeyboard}
        ><i></i><span>{formatTime(playheadUs, true)}</span></div>
      {/if}
    </div>

    <div
      bind:this={navigator}
      class="timeline-navigator"
      role="scrollbar"
      tabindex="0"
      aria-label="浏览完整视频"
      aria-orientation="horizontal"
      aria-controls="spycut-timeline-track"
      aria-valuemin="0"
      aria-valuemax={Math.max(0, durationUs - viewport.spanUs)}
      aria-valuenow={viewport.startUs}
      aria-valuetext={`${formatTime(viewport.startUs, true)} 到 ${formatTime(viewport.startUs + viewport.spanUs, true)}`}
      aria-disabled="false"
      on:pointerdown={(event) => beginNavigate(event, false)}
      on:keydown={navigateWithKeyboard}
    >
      <div class="navigator-base"></div>
      {#each intervals as interval}
        <i class="navigator-delete" style={`left:${globalPercent(interval.startUs)}%;width:${Math.max(.12, globalPercent(interval.endUs) - globalPercent(interval.startUs))}%`}></i>
      {/each}
      <div
        class="navigator-window"
        data-testid="navigator-window"
        style={`left:${navigatorLeft}%;width:${navigatorWidth}%`}
        role="presentation"
        on:pointerdown={(event) => beginNavigate(event, true)}
      ><span></span></div>
    </div>
  </div>
</section>
