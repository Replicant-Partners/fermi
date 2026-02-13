<script lang="ts">
  /**
   * Fermi Notebook Application
   *
   * Main entry point for the notebook interface.
   */

  import Notebook from './lib/components/Notebook.svelte';

  // For demo, hardcode a notebook ID
  // In production, this would come from router
  const notebookId = 'demo-notebook-1';
</script>

<main>
  <Notebook {notebookId} />
</main>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif;
    background: #1d2021;
    color: #ebdbb2;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  :global(*) {
    box-sizing: border-box;
  }

  :global(button) {
    font-family: inherit;
  }

  main {
    width: 100%;
    min-height: 100vh;
  }
</style>
