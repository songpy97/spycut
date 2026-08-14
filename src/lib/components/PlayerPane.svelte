<script lang="ts">
  import { createEventDispatcher, onDestroy } from "svelte";
  import { HtmlVideoAdapter } from "../player/HtmlVideoAdapter";

  export let sourceUrl = "";
  export let demo = false;
  export let playbackRate = 1;

  const dispatch = createEventDispatcher<{
    time: { playheadUs: number };
    state: { playing: boolean };
    error: { message: string };
    loaded: void;
  }>();

  let video: HTMLVideoElement;
  let adapter: HtmlVideoAdapter | null = null;
  let loadedSource = "";
  let frameRequest: number | null = null;
  let previewFrameRequest: number | null = null;
  let pendingPreviewUs: number | null = null;
  let loading = false;

  $: if (video && sourceUrl && sourceUrl !== loadedSource) void loadSource(sourceUrl);
  $: if (adapter) adapter.setRate(playbackRate);

  async function loadSource(url: string) {
    loading = true;
    cancelPreviewSeek();
    adapter?.dispose();
    adapter = new HtmlVideoAdapter(video);
    try {
      await adapter.load(url);
      loadedSource = url;
      adapter.setRate(playbackRate);
      dispatch("loaded");
      startFrameClock();
    } catch (error) {
      dispatch("error", { message: error instanceof Error ? error.message : "视频加载失败" });
    } finally {
      loading = false;
    }
  }

  function startFrameClock() {
    if (!("requestVideoFrameCallback" in video)) return;
    if (frameRequest !== null) video.cancelVideoFrameCallback(frameRequest);
    const update: VideoFrameRequestCallback = (_now, metadata) => {
      dispatch("time", { playheadUs: Math.round(metadata.mediaTime * 1_000_000) });
      frameRequest = video.requestVideoFrameCallback(update);
    };
    frameRequest = video.requestVideoFrameCallback(update);
  }

  function timeUpdate() {
    dispatch("time", { playheadUs: Math.round(video.currentTime * 1_000_000) });
  }

  export async function togglePlayback() {
    if (demo) return;
    if (!adapter) return;
    if (video.paused) await adapter.play(); else adapter.pause();
  }

  export function pause() {
    adapter?.pause();
  }

  export async function play() {
    if (!demo) await adapter?.play();
  }

  export async function seekTo(playheadUs: number): Promise<boolean> {
    cancelPreviewSeek();
    if (demo) {
      dispatch("time", { playheadUs });
      return true;
    }
    const activeAdapter = adapter;
    if (!activeAdapter) return false;
    const completed = await activeAdapter.seekTo(playheadUs / 1_000_000);
    if (!completed || adapter !== activeAdapter) return false;
    dispatch("time", { playheadUs });
    return true;
  }

  export function previewSeekTo(playheadUs: number) {
    if (demo || !adapter) return;
    pendingPreviewUs = Math.max(0, playheadUs);
    if (previewFrameRequest !== null) return;
    previewFrameRequest = requestAnimationFrame(() => {
      previewFrameRequest = null;
      if (pendingPreviewUs === null) return;
      const nextUs = pendingPreviewUs;
      pendingPreviewUs = null;
      adapter?.previewSeekTo(nextUs / 1_000_000);
    });
  }

  function cancelPreviewSeek() {
    if (previewFrameRequest !== null) cancelAnimationFrame(previewFrameRequest);
    previewFrameRequest = null;
    pendingPreviewUs = null;
  }

  onDestroy(() => {
    cancelPreviewSeek();
    if (frameRequest !== null && video) video.cancelVideoFrameCallback(frameRequest);
    adapter?.dispose();
  });
</script>

<section class="player-pane" class:demo-player={demo}>
  {#if demo}
    <div class="demo-screen" aria-label="演示视频区域">
      <div class="code-window">
        <div class="code-bar"><span></span><span></span><span></span><b>树上倍增 · 直播课</b></div>
        <pre><small>01</small> <em>const</em> int LOG = 20;
<small>02</small> vector&lt;int&gt; edge[N];
<small>03</small>
<small>04</small> <em>void</em> dfs(int u, int fa) &#123;
<small>05</small>   depth[u] = depth[fa] + 1;
<small>06</small>   up[u][0] = fa;
<small>07</small>   <strong>for</strong> (int i = 1; i &lt; LOG; ++i)
<small>08</small>     up[u][i] = up[up[u][i-1]][i-1];
<small>09</small> &#125;</pre>
      </div>
      <div class="demo-cursor">×</div>
      <div class="demo-caption">先确定状态，再写转移。</div>
    </div>
  {:else}
    <video
      bind:this={video}
      playsinline
      preload="metadata"
      on:timeupdate={timeUpdate}
      on:play={() => dispatch("state", { playing: true })}
      on:pause={() => dispatch("state", { playing: false })}
    ></video>
    {#if loading}<div class="video-loading"><span class="spinner"></span>正在建立播放通道</div>{/if}
  {/if}
  <div class="player-corner top-left"></div><div class="player-corner top-right"></div>
</section>
