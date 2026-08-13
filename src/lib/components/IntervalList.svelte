<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { DeleteInterval } from "../types/contracts";
  import { formatDurationCompact, formatTime } from "../utils/time";

  export let intervals: DeleteInterval[] = [];
  export let reviewedIds: number[] = [];
  export let selectedId: number | null = null;
  export let locked = false;

  const dispatch = createEventDispatcher<{
    select: { id: number }; remove: { id: number }; reviewed: { id: number; reviewed: boolean };
  }>();
</script>

<aside class="interval-panel">
  <header>
    <div><p>DELETE LOG</p><h2>删除区间</h2></div>
    <span>{String(intervals.length).padStart(2, "0")}</span>
  </header>
  {#if intervals.length === 0}
    <div class="no-intervals"><b>还没有标记</b><p>播放到不公开内容开头按 I，结束时按 O。</p></div>
  {:else}
    <ol class="interval-list">
      {#each intervals as interval, index}
        <li class:selected={interval.id === selectedId}>
          <button class="interval-main" type="button" on:click={() => dispatch("select", { id: interval.id })}>
            <span class="interval-index">{String(index + 1).padStart(2, "0")}</span>
            <span class="interval-time"><strong>{formatTime(interval.startUs)}</strong><i>→</i><strong>{formatTime(interval.endUs)}</strong><small>{formatDurationCompact(interval.endUs - interval.startUs)}</small></span>
          </button>
          <button
            type="button"
            class="review-dot"
            class:reviewed={reviewedIds.includes(interval.id)}
            disabled={locked}
            title={reviewedIds.includes(interval.id) ? "已复核" : "标记为已复核"}
            on:click={() => dispatch("reviewed", { id: interval.id, reviewed: !reviewedIds.includes(interval.id) })}
          >✓</button>
          <button class="remove-interval" type="button" disabled={locked} title="移除此删除标记" on:click={() => dispatch("remove", { id: interval.id })}>×</button>
        </li>
      {/each}
    </ol>
  {/if}
  <footer><span><i></i>已复核 {reviewedIds.length}</span><span>未复核 {Math.max(0, intervals.length - reviewedIds.length)}</span></footer>
</aside>
