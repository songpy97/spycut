<script lang="ts">
  import { createEventDispatcher } from "svelte";
  export let message: string;
  const dispatch = createEventDispatcher<{ dismiss: void }>();
</script>

<aside class="error-banner" role="alert">
  <div class="error-mark">!</div>
  <div>
    <strong>操作未完成</strong>
    <p>{message}</p>
  </div>
  <button type="button" on:click={() => dispatch("dismiss")} aria-label="关闭错误提示">×</button>
</aside>

