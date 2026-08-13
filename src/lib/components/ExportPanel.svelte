<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { ExportProgress, ExportResult, ExportStarted } from "../types/contracts";
  import { formatTime } from "../utils/time";

  export let preparing = false;
  export let started: ExportStarted | null = null;
  export let progress: ExportProgress | null = null;
  export let result: ExportResult | null = null;

  const dispatch = createEventDispatcher<{ cancel: void; close: void; reveal: { path: string } }>();
  $: percent = result?.status === "completed" ? 100 : Math.max(1.5, progress?.percent ?? (preparing ? 1.5 : 0));
  $: cancellable = Boolean(started && !result && (!progress || ["preparing", "encoding"].includes(progress.phase)));
  $: phaseTitle = result
    ? result.status === "completed" ? "公开版已经就绪" : result.status === "cancelled" ? "导出已取消" : "导出没有完成"
    : progress?.phase === "validating" ? "正在自动验收"
    : progress?.phase === "finalizing" ? "正在提交成片"
    : preparing ? "正在准备精确导出" : "正在重建公开版视频";
</script>

<div class="export-layer" role="dialog" aria-modal="true" aria-labelledby="export-title">
  <section class:success={result?.status === "completed"} class:failed={result?.status === "failed"} class="export-sheet">
    <div class="export-kicker"><span></span> FRAME-ACCURATE EXPORT</div>
    <h2 id="export-title">{phaseTitle}</h2>
    <p class="export-message">{result?.message ?? progress?.message ?? "正在校验源文件、磁盘空间和可用硬件编码器"}</p>

    <div class="export-progress-track" aria-label="导出进度" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(percent)} role="progressbar">
      <i style={`width:${percent}%`}></i>
    </div>
    <div class="export-progress-meta">
      <strong>{Math.round(percent)}%</strong>
      {#if progress?.speed}<span>{progress.speed}</span>{/if}
      {#if progress && progress.sourceDurationUs > 0}<span>已扫描 {formatTime(progress.processedSourceUs)} / {formatTime(progress.sourceDurationUs)}</span>{/if}
    </div>

    {#if started}
      <dl class="export-facts">
        <div><dt>编码器</dt><dd>{started.encoder.displayName}</dd></div>
        <div><dt>方式</dt><dd>{started.encoder.hardwareAccelerated ? "硬件加速 · 全量重建" : "软件编码 · 全量重建"}</dd></div>
        <div><dt>输出</dt><dd title={started.destination}>{started.destination}</dd></div>
      </dl>
    {/if}

    {#if result?.validation}
      <div class="validation-pass">
        <span>✓</span>
        <div><strong>自动验收通过</strong><p>时长误差 {(result.validation.durationDeltaUs / 1000).toFixed(0)} ms{#if result.validation.avDurationDeltaUs !== null} · 音画差 {(result.validation.avDurationDeltaUs / 1000).toFixed(0)} ms{/if} · 起始偏移 {(result.validation.startTimeUs / 1000).toFixed(0)} ms · 已解码检查 {result.validation.decodedCheckpoints} 个关键位置</p></div>
      </div>
    {:else if result?.status === "failed"}
      <pre class="export-diagnostics">{result.message}</pre>
    {/if}

    <footer class="export-actions">
      {#if result}
        <button type="button" class="secondary" on:click={() => dispatch("close")}>关闭</button>
        {#if result.status === "completed" && result.outputPath}
          <button type="button" class="primary" on:click={() => dispatch("reveal", { path: result.outputPath! })}>在文件夹中显示</button>
        {/if}
      {:else}
        <div><span class="spinner"></span><small>关闭软件会中断当前任务</small></div>
        <button type="button" class="secondary" disabled={!cancellable} on:click={() => dispatch("cancel")}>{cancellable ? "取消导出" : "正在完成验收…"}</button>
      {/if}
    </footer>
  </section>
</div>
