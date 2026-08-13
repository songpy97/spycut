<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { formatTime } from "../utils/time";

  export let playheadUs = 0;
  export let durationUs = 0;
  export let playing = false;
  export let playbackRate = 1;
  const dispatch = createEventDispatcher<{
    play: void; seek: { deltaUs: number }; rate: { value: number };
  }>();

  const rates = [0.5, 0.75, 1, 1.25, 1.5, 2];
</script>

<div class="transport">
  <div class="transport-group">
    <button type="button" class="icon-button" on:click={() => dispatch("seek", { deltaUs: -5_000_000 })} title="后退 5 秒">−5</button>
    <button type="button" class="play-button" on:click={() => dispatch("play")} aria-label={playing ? "暂停" : "播放"}>{playing ? "Ⅱ" : "▶"}</button>
    <button type="button" class="icon-button" on:click={() => dispatch("seek", { deltaUs: 5_000_000 })} title="前进 5 秒">+5</button>
    <div class="time-readout"><strong>{formatTime(playheadUs, true)}</strong><span>/ {formatTime(durationUs)}</span></div>
  </div>

  <div class="transport-group rate-strip" aria-label="播放速度">
    {#each rates as rate}
      <button type="button" class:active={rate === playbackRate} on:click={() => dispatch("rate", { value: rate })}>{rate}×</button>
    {/each}
  </div>
</div>
