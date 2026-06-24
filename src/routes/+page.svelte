<script lang="ts">
  const isLinux = () => {
    try {
      const ua = navigator.userAgent || '';
      return /linux|x11/i.test(ua) && !/android/i.test(ua);
    } catch {
      return false;
    }
  };

  const dragRegionEnabled = !isLinux();
</script>

<main>
  {#if dragRegionEnabled}
    <div class="drag-region" aria-hidden="true" data-tauri-drag-region></div>
  {/if}
  <h1>Codex Timeline</h1>
  <p>Local-first desktop tool for inspecting Codex agent runs.</p>
</main>

<style>
  :global(body) {
    margin: 0;
    font-family:
      Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    color: #1f2933;
    background: #f7f8f8;
  }

  main {
    display: flex;
    box-sizing: border-box;
    min-height: 100vh;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 32px 32px 96px;
    text-align: center;
  }

  .drag-region {
    position: fixed;
    top: 0;
    right: 0;
    left: 0;
    z-index: 10;
    height: 28px;
    user-select: none;
    -webkit-app-region: drag;
  }

  h1 {
    margin: 0 0 4px;
    font-size: 28px;
    line-height: 1.2;
  }

  p {
    margin: 0;
    color: #667085;
  }
</style>
