<script lang="ts">
  /**
   * Driver Cell Component
   *
   * Edits and visualizes a probabilistic driver (continuous/discrete/binary).
   * Displays distribution samples and statistics.
   */

  import type { DriverCell as DriverCellType, Distribution } from '../../types/notebook';
  import { updateCell, executeSingleCell } from '../../stores/notebook';

  export let cell: DriverCellType;
  export let readonly: boolean = false;

  let isEditing = false;

  function handleExecute() {
    executeSingleCell(cell.id);
  }

  function getDistributionLabel(dist: Distribution): string {
    switch (dist.type) {
      case 'uniform':
        return `Uniform(${dist.min}, ${dist.max})`;
      case 'normal':
        return `Normal(μ=${dist.mean}, σ=${dist.std})`;
      case 'triangular':
        return `Triangular(${dist.min}, ${dist.mode}, ${dist.max})`;
      case 'beta':
        return `Beta(α=${dist.alpha}, β=${dist.beta})`;
      case 'lognormal':
        return `LogNormal(μ=${dist.mu}, σ=${dist.sigma})`;
      default:
        return 'Unknown';
    }
  }
</script>

<div class="cell driver-cell">
  <!-- Cell Toolbar -->
  <div class="cell-toolbar">
    <div class="cell-header">
      <span class="cell-type">Driver</span>
      <span class="driver-name">{cell.name}</span>
      <span class="driver-type-badge {cell.driver_type}">{cell.driver_type}</span>
    </div>
    <div class="cell-actions">
      {#if !readonly}
        <button on:click={handleExecute} class="btn-small btn-primary">Sample</button>
      {/if}
    </div>
  </div>

  <!-- Distribution -->
  <div class="cell-content">
    <div class="distribution-info">
      <span class="label">Distribution:</span>
      <span class="value">{getDistributionLabel(cell.distribution)}</span>
      {#if cell.unit}
        <span class="unit">({cell.unit})</span>
      {/if}
    </div>

    {#if cell.rationale}
      <div class="rationale">
        <strong>Rationale:</strong>
        <p>{cell.rationale}</p>
      </div>
    {/if}
  </div>

  <!-- Output (Samples & Statistics) -->
  {#if cell.output}
    <div class="cell-output">
      <div class="stats-grid">
        <div class="stat">
          <span class="stat-label">Mean</span>
          <span class="stat-value">{cell.output.mean.toFixed(2)}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Std Dev</span>
          <span class="stat-value">{cell.output.std.toFixed(2)}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Median</span>
          <span class="stat-value">{cell.output.percentiles.p50.toFixed(2)}</span>
        </div>
      </div>

      <!-- Percentiles -->
      <div class="percentiles">
        <div class="percentile-bar">
          <span class="p-label">5th</span>
          <div class="bar-container">
            <div class="bar" style="width: 20%"></div>
          </div>
          <span class="p-value">{cell.output.percentiles.p5.toFixed(2)}</span>
        </div>
        <div class="percentile-bar">
          <span class="p-label">25th</span>
          <div class="bar-container">
            <div class="bar" style="width: 40%"></div>
          </div>
          <span class="p-value">{cell.output.percentiles.p25.toFixed(2)}</span>
        </div>
        <div class="percentile-bar highlight">
          <span class="p-label">50th</span>
          <div class="bar-container">
            <div class="bar" style="width: 60%"></div>
          </div>
          <span class="p-value">{cell.output.percentiles.p50.toFixed(2)}</span>
        </div>
        <div class="percentile-bar">
          <span class="p-label">75th</span>
          <div class="bar-container">
            <div class="bar" style="width: 80%"></div>
          </div>
          <span class="p-value">{cell.output.percentiles.p75.toFixed(2)}</span>
        </div>
        <div class="percentile-bar">
          <span class="p-label">95th</span>
          <div class="bar-container">
            <div class="bar" style="width: 100%"></div>
          </div>
          <span class="p-value">{cell.output.percentiles.p95.toFixed(2)}</span>
        </div>
      </div>

      <!-- Mini Histogram (ASCII-style) -->
      {#if cell.output.samples.length > 0}
        <div class="histogram-container">
          <div class="histogram">
            <!-- Simplified visualization - would use D3.js in production -->
            <div class="histogram-note">
              {cell.output.samples.length} samples generated
            </div>
          </div>
        </div>
      {/if}
    </div>
  {/if}

  <!-- Dependencies -->
  {#if cell.dependencies.length > 0}
    <div class="dependencies">
      <span class="dep-label">Depends on:</span>
      {#each cell.dependencies as depId}
        <span class="dep-badge">{depId}</span>
      {/each}
    </div>
  {/if}
</div>

<style>
  .driver-cell {
    background: #3c3836;
    border: 1px solid #504945;
    border-radius: 6px;
    padding: 16px;
    margin-bottom: 12px;
  }

  .cell-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
    padding-bottom: 8px;
    border-bottom: 1px solid #504945;
  }

  .cell-header {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .cell-type {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: #bdae93;
    font-weight: 500;
  }

  .driver-name {
    font-size: 14px;
    font-weight: 500;
    color: #fbf1c7;
    font-family: monospace;
  }

  .driver-type-badge {
    padding: 2px 8px;
    font-size: 10px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .driver-type-badge.continuous {
    background: #fe8019;
    color: #1d2021;
  }

  .driver-type-badge.discrete {
    background: #83a598;
    color: #1d2021;
  }

  .driver-type-badge.binary {
    background: #d3869b;
    color: #1d2021;
  }

  .cell-actions {
    display: flex;
    gap: 8px;
  }

  .btn-small {
    padding: 4px 12px;
    font-size: 12px;
    background: transparent;
    border: 1px solid #665c54;
    color: #ebdbb2;
    border-radius: 4px;
    cursor: pointer;
    font-weight: 400;
  }

  .btn-small:hover {
    background: #504945;
  }

  .btn-primary {
    background: #8ec07c;
    border-color: #8ec07c;
    color: #1d2021;
  }

  .btn-primary:hover {
    background: #b8bb26;
    border-color: #b8bb26;
  }

  .cell-content {
    margin-bottom: 12px;
  }

  .distribution-info {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 8px;
  }

  .distribution-info .label {
    font-size: 12px;
    color: #928374;
  }

  .distribution-info .value {
    font-size: 13px;
    color: #fe8019;
    font-family: monospace;
  }

  .distribution-info .unit {
    font-size: 12px;
    color: #bdae93;
    font-style: italic;
  }

  .rationale {
    margin-top: 8px;
    padding: 8px;
    background: #282828;
    border-radius: 4px;
  }

  .rationale strong {
    display: block;
    font-size: 11px;
    color: #bdae93;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 4px;
  }

  .rationale p {
    margin: 0;
    font-size: 13px;
    color: #d5c4a1;
    line-height: 1.5;
  }

  .cell-output {
    background: #282828;
    padding: 12px;
    border-radius: 4px;
  }

  .stats-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 16px;
    margin-bottom: 16px;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .stat-label {
    font-size: 10px;
    color: #928374;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .stat-value {
    font-size: 18px;
    color: #8ec07c;
    font-weight: 300;
  }

  .percentiles {
    margin-top: 12px;
  }

  .percentile-bar {
    display: grid;
    grid-template-columns: 40px 1fr 60px;
    gap: 8px;
    align-items: center;
    margin-bottom: 6px;
  }

  .percentile-bar.highlight .bar {
    background: #8ec07c;
  }

  .p-label {
    font-size: 11px;
    color: #928374;
    text-align: right;
  }

  .bar-container {
    background: #1d2021;
    height: 8px;
    border-radius: 2px;
    overflow: hidden;
  }

  .bar {
    height: 100%;
    background: #504945;
    transition: width 0.3s ease;
  }

  .p-value {
    font-size: 12px;
    color: #d5c4a1;
    font-family: monospace;
  }

  .histogram-container {
    margin-top: 16px;
    padding-top: 12px;
    border-top: 1px solid #3c3836;
  }

  .histogram-note {
    font-size: 11px;
    color: #928374;
    text-align: center;
  }

  .dependencies {
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid #504945;
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .dep-label {
    font-size: 11px;
    color: #928374;
  }

  .dep-badge {
    padding: 2px 8px;
    background: #504945;
    color: #d5c4a1;
    border-radius: 3px;
    font-size: 11px;
    font-family: monospace;
  }
</style>
