<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { formatTime } from "../utils/time";

  export let playheadUs = 0;
  export let durationUs = 0;
  export let frameDurationUs = 1;
  export let pendingStartUs: number | null = null;
  export let canUndo = false;
  export let canRedo = false;
  export let markSaving = false;
  export let editLocked = false;
  export let zoomValue = 0;

  const dispatch = createEventDispatcher<{
    markStart: void;
    markEnd: void;
    cancelMark: void;
    undo: void;
    redo: void;
    fit: void;
    zoomStep: { direction: number };
    zoomValue: { value: number };
  }>();

  $: endIsValid = pendingStartUs !== null && playheadUs - pendingStartUs >= Math.max(1, frameDurationUs);
  $: editsDisabled = editLocked || markSaving;
  $: statusMessage = editLocked
    ? "导出期间区间编辑已锁定"
    : markSaving
      ? "正在保存删除区间…"
      : pendingStartUs !== null && !endIsValid
        ? "请将播放头移到删除起点之后"
        : pendingStartUs !== null
          ? `删除起点 ${formatTime(pendingStartUs, true)} 已设置`
          : "拖动播放头定位，再设置删除起点和终点";
</script>

<div class="timeline-toolbar" aria-label="时间轴工具栏">
  <div class="timeline-history-actions">
    <button type="button" disabled={!canUndo || editsDisabled} on:click={() => dispatch("undo")} aria-label="撤销">↶</button>
    <button type="button" disabled={!canRedo || editsDisabled} on:click={() => dispatch("redo")} aria-label="重做">↷</button>
  </div>
  <span class="toolbar-divider" aria-hidden="true"></span>
  <div class="timeline-mark-actions">
    <button
      type="button"
      class:armed={pendingStartUs !== null}
      disabled={editsDisabled}
      on:click={() => dispatch("markStart")}
      aria-label={pendingStartUs === null ? "设删除起点" : "重设删除起点"}
    >
      <span class="long-label">{pendingStartUs === null ? "设删除起点" : "重设删除起点"}</span>
      <span class="short-label">{pendingStartUs === null ? "设起点" : "重设起点"}</span><kbd>I</kbd>
    </button>
    <button
      type="button"
      class="finish-mark"
      disabled={editsDisabled || !endIsValid}
      on:click={() => dispatch("markEnd")}
      aria-label={markSaving ? "保存删除区间" : "完成删除区间"}
    >
      <span class="long-label">{markSaving ? "正在保存" : "完成删除区间"}</span>
      <span class="short-label">{markSaving ? "保存中" : "设终点"}</span><kbd>O</kbd>
    </button>
    {#if pendingStartUs !== null}
      <button type="button" class="cancel-mark" disabled={editsDisabled} on:click={() => dispatch("cancelMark")} aria-label="取消标记">取消 <kbd>Esc</kbd></button>
    {/if}
  </div>
  <div class="timeline-status" class:warning={pendingStartUs !== null && !endIsValid} aria-live="polite">{statusMessage}</div>
  <div class="timeline-time-readout"><strong>{formatTime(playheadUs, true)}</strong><span>/ {formatTime(durationUs)}</span></div>
  <div class="timeline-zoom-actions">
    <span class="snap-state"><i></i>帧吸附</span>
    <button type="button" class="fit-button" on:click={() => dispatch("fit")}>适配全片</button>
    <button type="button" on:click={() => dispatch("zoomStep", { direction: -1 })} aria-label="缩小时间轴">−</button>
    <input
      type="range"
      min="0"
      max="1"
      step="0.001"
      value={zoomValue}
      aria-label="时间轴缩放"
      on:input={(event) => dispatch("zoomValue", { value: Number((event.currentTarget as HTMLInputElement).value) })}
    />
    <button type="button" on:click={() => dispatch("zoomStep", { direction: 1 })} aria-label="放大时间轴">＋</button>
  </div>
</div>
