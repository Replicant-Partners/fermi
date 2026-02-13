/**
 * Notebook Store
 *
 * Reactive state management for notebooks using Svelte stores.
 * Handles dependency graph updates and cell re-execution.
 */

import { writable, derived, get } from 'svelte/store';
import type { Writable, Readable } from 'svelte/store';
import type { Notebook, Cell, DependencyGraph, ExecutionState } from '../types/notebook';
import { executeCell, executeNotebook } from '../api/client';

// ─── Current Notebook Store ─────────────────────────────────────────

export const currentNotebook: Writable<Notebook | null> = writable(null);
export const executionState: Writable<ExecutionState> = writable({
  status: 'idle',
  completed_cells: [],
});

// ─── Cells Store (Indexed by ID) ────────────────────────────────────

export const cells: Readable<Map<string, Cell>> = derived(
  currentNotebook,
  ($notebook) => {
    if (!$notebook) return new Map();

    const cellMap = new Map<string, Cell>();
    for (const cell of $notebook.cells) {
      cellMap.set(cell.id, cell);
    }
    return cellMap;
  }
);

// ─── Dependency Graph ───────────────────────────────────────────────

export const dependencyGraph: Readable<DependencyGraph> = derived(
  currentNotebook,
  ($notebook) => {
    if (!$notebook) return { nodes: [], edges: [] };
    return $notebook.dependency_graph;
  }
);

// ─── Helper: Get Dependent Cells ───────────────────────────────────

export function getDependents(cellId: string): string[] {
  const graph = get(dependencyGraph);
  return graph.edges
    .filter(edge => edge.from === cellId)
    .map(edge => edge.to);
}

// ─── Helper: Get Dependencies ───────────────────────────────────────

export function getDependencies(cellId: string): string[] {
  const graph = get(dependencyGraph);
  return graph.edges
    .filter(edge => edge.to === cellId)
    .map(edge => edge.from);
}

// ─── Helper: Topological Sort (Execution Order) ─────────────────────

export function getExecutionOrder(cellIds: string[]): string[] {
  const graph = get(dependencyGraph);
  const visited = new Set<string>();
  const result: string[] = [];

  function visit(id: string) {
    if (visited.has(id)) return;
    visited.add(id);

    const deps = getDependencies(id);
    for (const dep of deps) {
      visit(dep);
    }

    result.push(id);
  }

  for (const id of cellIds) {
    visit(id);
  }

  return result;
}

// ─── Actions: Update Cell ───────────────────────────────────────────

export async function updateCell(cellId: string, updates: Partial<Cell>) {
  currentNotebook.update($notebook => {
    if (!$notebook) return $notebook;

    const cellIndex = $notebook.cells.findIndex(c => c.id === cellId);
    if (cellIndex === -1) return $notebook;

    $notebook.cells[cellIndex] = {
      ...$notebook.cells[cellIndex],
      ...updates,
      updated_at: new Date().toISOString(),
    };

    return $notebook;
  });
}

// ─── Actions: Add Cell ──────────────────────────────────────────────

export function addCell(cell: Cell, afterCellId?: string) {
  currentNotebook.update($notebook => {
    if (!$notebook) return $notebook;

    const insertIndex = afterCellId
      ? $notebook.cells.findIndex(c => c.id === afterCellId) + 1
      : $notebook.cells.length;

    $notebook.cells.splice(insertIndex, 0, cell);

    // Reorder
    $notebook.cells.forEach((c, i) => {
      c.order = i;
    });

    return $notebook;
  });
}

// ─── Actions: Delete Cell ───────────────────────────────────────────

export function deleteCell(cellId: string) {
  currentNotebook.update($notebook => {
    if (!$notebook) return $notebook;

    $notebook.cells = $notebook.cells.filter(c => c.id !== cellId);

    // Reorder
    $notebook.cells.forEach((c, i) => {
      c.order = i;
    });

    // Remove from dependency graph
    $notebook.dependency_graph.nodes = $notebook.dependency_graph.nodes.filter(n => n !== cellId);
    $notebook.dependency_graph.edges = $notebook.dependency_graph.edges.filter(
      e => e.from !== cellId && e.to !== cellId
    );

    return $notebook;
  });
}

// ─── Actions: Execute Single Cell ──────────────────────────────────

export async function executeSingleCell(cellId: string) {
  const $notebook = get(currentNotebook);
  if (!$notebook) return;

  executionState.set({
    status: 'running',
    current_cell: cellId,
    completed_cells: [],
  });

  try {
    // Gather dependency values
    const deps = getDependencies(cellId);
    const $cells = get(cells);
    const dependencies: Record<string, any> = {};

    for (const depId of deps) {
      const depCell = $cells.get(depId);
      if (depCell && 'output' in depCell) {
        dependencies[depId] = depCell.output;
      }
    }

    // Execute cell
    const response = await executeCell($notebook.id, cellId, {
      cell_id: cellId,
      dependencies,
    });

    // Update cell output
    await updateCell(cellId, { output: response.output } as Partial<Cell>);

    // Execute dependents
    for (const depId of response.updated_cells) {
      await executeSingleCell(depId);
    }

    executionState.update($state => ({
      ...$state,
      status: 'success',
      completed_cells: [...$state.completed_cells, cellId],
    }));
  } catch (error) {
    executionState.set({
      status: 'error',
      current_cell: cellId,
      completed_cells: [],
      error_message: error instanceof Error ? error.message : 'Unknown error',
    });
  }
}

// ─── Actions: Execute All Cells ─────────────────────────────────────

export async function executeAllCells() {
  const $notebook = get(currentNotebook);
  if (!$notebook) return;

  executionState.set({
    status: 'running',
    completed_cells: [],
  });

  try {
    const response = await executeNotebook($notebook.id, {
      iterations: 10000,
    });

    // Update all cell outputs
    const $cells = get(cells);
    for (const [cellId, output] of Object.entries(response.cells)) {
      const cell = $cells.get(cellId);
      if (cell) {
        await updateCell(cellId, { output } as Partial<Cell>);
      }
    }

    executionState.set({
      status: 'success',
      completed_cells: Array.from($cells.keys()),
    });
  } catch (error) {
    executionState.set({
      status: 'error',
      completed_cells: [],
      error_message: error instanceof Error ? error.message : 'Unknown error',
    });
  }
}

// ─── Actions: Rebuild Dependency Graph ─────────────────────────────

export function rebuildDependencyGraph() {
  currentNotebook.update($notebook => {
    if (!$notebook) return $notebook;

    const nodes: string[] = [];
    const edges: DependencyGraph['edges'] = [];

    for (const cell of $notebook.cells) {
      nodes.push(cell.id);

      if ('dependencies' in cell && Array.isArray(cell.dependencies)) {
        for (const depId of cell.dependencies) {
          edges.push({
            from: depId,
            to: cell.id,
            type: 'data',
          });
        }
      }
    }

    $notebook.dependency_graph = { nodes, edges };
    return $notebook;
  });
}
