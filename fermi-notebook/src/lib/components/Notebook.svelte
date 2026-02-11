<script lang="ts">
  /**
   * Notebook Component
   *
   * Main orchestrator for the Fermi notebook interface.
   * Manages cell rendering, execution flow, and agent assistance.
   */

  import { onMount } from 'svelte';
  import type { Cell } from '../types/notebook';
  import { currentNotebook, cells, executionState, executeAllCells } from '../stores/notebook';
  import { getNotebook } from '../api/client';

  import QuestionCell from './cells/QuestionCell.svelte';
  import DriverCell from './cells/DriverCell.svelte';
  // import ModelCell from './cells/ModelCell.svelte';
  // import MarkdownCell from './cells/MarkdownCell.svelte';

  export let notebookId: string;

  let loading = true;
  let error: string | null = null;

  onMount(async () => {
    try {
      const notebook = await getNotebook(notebookId);
      currentNotebook.set(notebook);
      loading = false;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load notebook';
      loading = false;
    }
  });

  function handleRunAll() {
    executeAllCells();
  }

  function getCellComponent(cell: Cell) {
    switch (cell.type) {
      case 'question':
        return QuestionCell;
      case 'driver':
        return DriverCell;
      // case 'model':
      //   return ModelCell;
      // case 'markdown':
      //   return MarkdownCell;
      default:
        return null;
    }
  }
</script>

<div class="notebook-container">
  {#if loading}
    <div class="loading-state">
      <div class="spinner"></div>
      <p>Loading notebook...</p>
    </div>
  {:else if error}
    <div class="error-state">
      <p class="error-message">{error}</p>
      <button on:click={() => window.location.reload()}>Retry</button>
    </div>
  {:else if $currentNotebook}
    <!-- Notebook Header -->
    <header class="notebook-header">
      <div class="header-left">
        <h1 class="notebook-title">{$currentNotebook.title}</h1>
        {#if $currentNotebook.description}
          <p class="notebook-description">{$currentNotebook.description}</p>
        {/if}
      </div>
      <div class="header-right">
        <button
          on:click={handleRunAll}
          class="btn-run-all"
          disabled={$executionState.status === 'running'}
        >
          {#if $executionState.status === 'running'}
            Running...
          {:else}
            ▶ Run All
          {/if}
        </button>
        <div class="notebook-meta">
          <span class="version">v{$currentNotebook.metadata.version}</span>
          <span class="visibility-badge {$currentNotebook.permissions.visibility}">
            {$currentNotebook.permissions.visibility}
          </span>
        </div>
      </div>
    </header>

    <!-- Execution Status Bar -->
    {#if $executionState.status === 'running'}
      <div class="execution-status running">
        <div class="status-bar">
          <div class="status-progress"></div>
        </div>
        <p class="status-text">
          Executing {$executionState.current_cell || 'cells'}...
        </p>
      </div>
    {:else if $executionState.status === 'error'}
      <div class="execution-status error">
        <p class="status-text">❌ {$executionState.error_message}</p>
      </div>
    {:else if $executionState.status === 'success'}
      <div class="execution-status success">
        <p class="status-text">
          ✓ Executed {$executionState.completed_cells.length} cells
        </p>
      </div>
    {/if}

    <!-- Cells Grid -->
    <div class="cells-container">
      {#each $currentNotebook.cells as cell (cell.id)}
        {@const Component = getCellComponent(cell)}
        {#if Component}
          <svelte:component this={Component} {cell} />
        {:else}
          <div class="unsupported-cell">
            <p>Unsupported cell type: {cell.type}</p>
          </div>
        {/if}
      {/each}
    </div>

    <!-- Add Cell Button -->
    <div class="add-cell-area">
      <button class="btn-add-cell">+ Add Cell</button>
    </div>
  {/if}
</div>

<style>
  .notebook-container {
    max-width: 900px;
    margin: 0 auto;
    padding: 24px;
    background: #1d2021;
    min-height: 100vh;
  }

  /* Loading & Error States */
  .loading-state,
  .error-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 60vh;
    color: #d5c4a1;
  }

  .spinner {
    width: 40px;
    height: 40px;
    border: 3px solid #504945;
    border-top-color: #8ec07c;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .error-message {
    color: #fb4934;
    margin-bottom: 16px;
  }

  /* Notebook Header */
  .notebook-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 32px;
    padding-bottom: 24px;
    border-bottom: 2px solid #504945;
  }

  .header-left {
    flex: 1;
  }

  .notebook-title {
    font-size: 28px;
    font-weight: 300;
    color: #fbf1c7;
    margin: 0 0 8px 0;
    letter-spacing: -0.5px;
  }

  .notebook-description {
    font-size: 14px;
    color: #bdae93;
    margin: 0;
    line-height: 1.5;
  }

  .header-right {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 12px;
  }

  .btn-run-all {
    padding: 10px 24px;
    background: #8ec07c;
    border: none;
    color: #1d2021;
    border-radius: 4px;
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn-run-all:hover:not(:disabled) {
    background: #b8bb26;
  }

  .btn-run-all:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .notebook-meta {
    display: flex;
    gap: 12px;
    align-items: center;
  }

  .version {
    font-size: 12px;
    color: #928374;
    font-family: monospace;
  }

  .visibility-badge {
    padding: 3px 10px;
    font-size: 11px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .visibility-badge.private {
    background: #fb4934;
    color: #1d2021;
  }

  .visibility-badge.shared {
    background: #fe8019;
    color: #1d2021;
  }

  .visibility-badge.public {
    background: #8ec07c;
    color: #1d2021;
  }

  /* Execution Status */
  .execution-status {
    margin-bottom: 24px;
    padding: 12px 16px;
    border-radius: 4px;
  }

  .execution-status.running {
    background: #fe8019;
    color: #1d2021;
  }

  .execution-status.error {
    background: #fb4934;
    color: #1d2021;
  }

  .execution-status.success {
    background: #8ec07c;
    color: #1d2021;
  }

  .status-bar {
    height: 4px;
    background: rgba(0, 0, 0, 0.2);
    border-radius: 2px;
    overflow: hidden;
    margin-bottom: 8px;
  }

  .status-progress {
    height: 100%;
    background: rgba(0, 0, 0, 0.4);
    animation: progress 1.5s ease-in-out infinite;
  }

  @keyframes progress {
    0% { width: 0%; }
    50% { width: 70%; }
    100% { width: 100%; }
  }

  .status-text {
    margin: 0;
    font-size: 13px;
    font-weight: 500;
  }

  /* Cells Container */
  .cells-container {
    margin-bottom: 24px;
  }

  .unsupported-cell {
    background: #3c3836;
    border: 1px dashed #665c54;
    border-radius: 6px;
    padding: 24px;
    margin-bottom: 12px;
    text-align: center;
    color: #928374;
  }

  /* Add Cell */
  .add-cell-area {
    display: flex;
    justify-content: center;
    padding: 24px 0;
  }

  .btn-add-cell {
    padding: 12px 32px;
    background: transparent;
    border: 2px dashed #665c54;
    color: #bdae93;
    border-radius: 6px;
    font-size: 14px;
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn-add-cell:hover {
    border-color: #8ec07c;
    color: #8ec07c;
    background: rgba(142, 192, 124, 0.05);
  }
</style>
