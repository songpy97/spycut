<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { SessionProjection } from "../types/contracts";
  import { formatBytes, formatDurationCompact, formatTime } from "../utils/time";

  export let session: SessionProjection;
  export let activeIndex = 0;

  const dispatch = createEventDispatcher<{
    close: void; navigate: { index: number }; preview: { index: number }; reviewed: { id: number; reviewed: boolean };
    export: { allowUnreviewed: boolean; allowBitDepthFallback: boolean };
  }>();

  let overrideArmed = false;
  let previousReviewSignature = "";

  $: intervals = session.project.deleteIntervals;
  $: reviewed = session.project.reviewedIntervalIds;
  $: current = intervals[activeIndex];
  $: expectedBytes = session.project.source.sizeBytes * (session.keptDurationUs / session.project.media.durationUs) * 1.12;
  $: allReviewed = intervals.length > 0 && intervals.every((item) => reviewed.includes(item.id));
  $: unreviewedCount = Math.max(0, intervals.length - reviewed.length);
  $: needsBitDepthFallback = (session.project.media.bitDepth ?? 8) > 8;
  $: needsOverride = !allReviewed || needsBitDepthFallback;
  $: overrideReason = [
    !allReviewed ? `${unreviewedCount} 处未复核` : "",
    needsBitDepthFallback ? "10-bit 将转为 8-bit" : ""
  ].filter(Boolean).join("，");
  $: {
    const signature = `${intervals.map((item) => item.id).join(",")}|${reviewed.join(",")}`;
    if (signature !== previousReviewSignature) {
      previousReviewSignature = signature;
      overrideArmed = false;
    }
  }

  function requestExport() {
    if (!needsOverride) {
      dispatch("export", { allowUnreviewed: false, allowBitDepthFallback: false });
    } else if (overrideArmed) {
      dispatch("export", {
        allowUnreviewed: !allReviewed,
        allowBitDepthFallback: needsBitDepthFallback
      });
    } else {
      overrideArmed = true;
    }
  }
</script>

<div class="review-layer" role="dialog" aria-modal="true" aria-labelledby="review-title">
  <section class="review-sheet">
    <header class="review-header">
      <div><p>FINAL CHECK</p><h2 id="review-title">公开前，逐处确认。</h2><span>每个红色区间都会从成片中消失。修改标记后，该处需要重新复核。</span></div>
      <button type="button" on:click={() => dispatch("close")} aria-label="返回编辑">×</button>
    </header>

    <div class="review-metrics">
      <article><span>源视频</span><strong>{formatDurationCompact(session.project.media.durationUs)}</strong><small>{formatBytes(session.project.source.sizeBytes)}</small></article>
      <article class="removed"><span>删除</span><strong>− {formatDurationCompact(session.deletedDurationUs)}</strong><small>{intervals.length} 个区间</small></article>
      <article class="result"><span>预计成片</span><strong>{formatDurationCompact(session.keptDurationUs)}</strong><small>约 {formatBytes(expectedBytes)}</small></article>
      <article><span>精度</span><strong>≤ 1 帧</strong><small>{session.project.media.frameRate.num / session.project.media.frameRate.den} fps</small></article>
    </div>

    {#if session.project.media.variableFrameRate || session.project.media.videoStreamCount > 1 || session.project.media.audioStreamCount > 1 || (session.project.media.hasAudio && session.project.media.audioCodec !== "aac") || needsBitDepthFallback}
      <div class="compatibility-warnings">
        {#if session.project.media.variableFrameRate}<p>可变帧率源将统一输出为 {(session.project.media.frameRate.num / session.project.media.frameRate.den).toFixed(3)} fps CFR。</p>{/if}
        {#if session.project.media.videoStreamCount > 1}<p>检测到 {session.project.media.videoStreamCount} 条视频流，只使用第一条。</p>{/if}
        {#if session.project.media.audioStreamCount > 1}<p>检测到 {session.project.media.audioStreamCount} 条音频流，只使用第一条。</p>{/if}
        {#if session.project.media.hasAudio && session.project.media.audioCodec !== "aac"}<p>{session.project.media.audioCodec ?? "未知"} 音频将重新编码为 AAC。</p>{/if}
        {#if needsBitDepthFallback}<p class="requires-confirmation">源视频是 {session.project.media.bitDepth}-bit；V1 会转为 8-bit，导出前需要再次确认。</p>{/if}
      </div>
    {/if}

    {#if current}
      <div class="join-review">
        <div class="join-counter"><span>JOIN</span><strong>{String(activeIndex + 1).padStart(2, "0")}</strong><small>/ {String(intervals.length).padStart(2, "0")}</small></div>
        <div class="join-times">
          <div><span>保留至</span><strong>{formatTime(current.startUs, true)}</strong></div>
          <i>→</i>
          <div><span>从这里继续</span><strong>{formatTime(current.endUs, true)}</strong></div>
        </div>
        <button class="preview-join" type="button" on:click={() => dispatch("preview", { index: activeIndex })}>▶ 试听试看连接点</button>
        <button
          class="confirm-join"
          class:checked={reviewed.includes(current.id)}
          type="button"
          on:click={() => dispatch("reviewed", { id: current.id, reviewed: !reviewed.includes(current.id) })}
        >{reviewed.includes(current.id) ? "✓ 已确认公开安全" : "确认这个连接点"}</button>
      </div>

      <div class="join-navigator">
        <button type="button" disabled={activeIndex === 0} on:click={() => dispatch("navigate", { index: activeIndex - 1 })}>← 上一处</button>
        <div>{#each intervals as item, index}<button type="button" class:active={index === activeIndex} class:done={reviewed.includes(item.id)} on:click={() => dispatch("navigate", { index })}>{index + 1}</button>{/each}</div>
        <button type="button" disabled={activeIndex >= intervals.length - 1} on:click={() => dispatch("navigate", { index: activeIndex + 1 })}>下一处 →</button>
      </div>
    {:else}
      <div class="review-empty">还没有删除区间。无需裁切时，请直接使用原视频。</div>
    {/if}

    <footer class="review-footer">
      <div><span class:complete={allReviewed && !needsBitDepthFallback}></span><strong>{reviewed.length} / {intervals.length} 已复核</strong><small>{!needsOverride ? "可以开始精确导出" : overrideArmed ? `${overrideReason}，请再次确认` : overrideReason}</small></div>
      <button type="button" class="secondary" on:click={() => dispatch("close")}>返回调整</button>
      <button type="button" class="primary export-ready" disabled={intervals.length === 0} on:click={requestExport}>
        {!needsOverride ? "选择位置并导出 MP4" : overrideArmed ? "确认仍然导出" : `需要确认：${overrideReason}`}
      </button>
    </footer>
  </section>
</div>
