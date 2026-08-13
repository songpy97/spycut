<script lang="ts">
  import { createEventDispatcher } from "svelte";
  export let loading = false;
  const dispatch = createEventDispatcher<{ open: void; demo: void }>();
</script>

<main class="empty-workspace">
  <section class="empty-copy">
    <div class="brand-lockup">
      <div class="brand-slash" aria-hidden="true"><i></i><i></i></div>
      <span>SPYCUT / FRAME-ACCURATE</span>
    </div>
    <p class="chapter">LECTURE CUTTER · 01</p>
    <h1>只删不该公开的，<br /><em>其余一帧不乱。</em></h1>
    <p class="intro">面向长时间直播录屏的精确区间工具。没有可移动片段，没有波纹剪辑，原始顺序永远锁定。</p>
    <div class="empty-actions">
      <button class="primary large" type="button" disabled={loading} on:click={() => dispatch("open")}>
        {#if loading}<span class="spinner"></span>正在读取媒体…{:else}打开 MP4 录屏 <kbd>⌘O</kbd>{/if}
      </button>
      {#if !loading}
        <button class="text-action" type="button" on:click={() => dispatch("demo")}>查看界面演示</button>
      {/if}
    </div>
  </section>

  <aside class="empty-manifesto" aria-label="工作原则">
    <div class="manifesto-number">03:12:25</div>
    <div class="manifesto-track"><span style="width: 18%"></span><b style="left: 34%; width: 9%"></b><span style="left: 51%; width: 30%"></span></div>
    <dl>
      <div><dt>01</dt><dd><strong>原文件只读</strong><span>不复制，不覆盖，不修改</span></dd></div>
      <div><dt>02</dt><dd><strong>只记录删除区间</strong><span>不存在能被误拖动的片段</span></dd></div>
      <div><dt>03</dt><dd><strong>逐连接点复核</strong><span>隐私内容离开前再确认一次</span></dd></div>
    </dl>
  </aside>
</main>

