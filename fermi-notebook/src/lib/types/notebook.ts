/**
 * Fermi Notebook Type System
 *
 * Strict TypeScript definitions for FPL cells and notebooks.
 * Enforces type safety across the entire notebook system.
 */

// ─── Visibility & Permissions ───────────────────────────────────────

export type NotebookVisibility = 'private' | 'shared' | 'public';

export interface NotebookPermissions {
  visibility: NotebookVisibility;
  owner_id: string;
  team_id?: string;
  org_id?: string;
  collaborators?: string[];
}

// ─── Core Cell Types ────────────────────────────────────────────────

export type CellType =
  | 'question'
  | 'base_rate'
  | 'driver'
  | 'evidence'
  | 'model'
  | 'simulation'
  | 'markdown'
  | 'visualization';

export interface BaseCell {
  id: string;
  type: CellType;
  order: number;
  created_at: string;
  updated_at: string;
}

// ─── Question Cell ──────────────────────────────────────────────────

export interface QuestionCell extends BaseCell {
  type: 'question';
  text: string;
  base_rate?: BaseRate;
  output?: {
    probability: number;
    confidence_interval: [number, number];
    brier_score?: number;
  };
}

export interface BaseRate {
  reference_class: string;
  historical_frequency: number;
  sample_size: number;
  source: string;
  reasoning: string;
  generated_by: 'human' | 'agent';
}

// ─── Driver Cell ────────────────────────────────────────────────────

export type DriverType = 'continuous' | 'discrete' | 'binary';

export type Distribution =
  | UniformDistribution
  | NormalDistribution
  | TriangularDistribution
  | BetaDistribution
  | LogNormalDistribution;

export interface UniformDistribution {
  type: 'uniform';
  min: number;
  max: number;
}

export interface NormalDistribution {
  type: 'normal';
  mean: number;
  std: number;
}

export interface TriangularDistribution {
  type: 'triangular';
  min: number;
  mode: number;
  max: number;
}

export interface BetaDistribution {
  type: 'beta';
  alpha: number;
  beta: number;
}

export interface LogNormalDistribution {
  type: 'lognormal';
  mu: number;
  sigma: number;
}

export interface DriverCell extends BaseCell {
  type: 'driver';
  name: string;
  driver_type: DriverType;
  distribution: Distribution;
  unit?: string;
  rationale: string;
  dependencies: string[]; // Cell IDs this driver depends on
  output?: {
    samples: number[];
    mean: number;
    std: number;
    percentiles: {
      p5: number;
      p25: number;
      p50: number;
      p75: number;
      p95: number;
    };
  };
}

// ─── Evidence Cell ──────────────────────────────────────────────────

export interface EvidenceCell extends BaseCell {
  type: 'evidence';
  driver_id: string;
  evidence_type: 'update' | 'constraint' | 'observation';
  value: number | boolean | [number, number];
  strength: number; // 0.0 to 1.0
  source: string;
  reasoning: string;
  dependencies: string[];
}

// ─── Model Cell ─────────────────────────────────────────────────────

export interface ModelCell extends BaseCell {
  type: 'model';
  expression: string; // FPL expression
  ast?: any; // Parsed AST from backend
  dependencies: string[];
  output?: {
    formula: string;
    evaluation?: any;
  };
}

// ─── Simulation Cell ────────────────────────────────────────────────

export interface SimulationCell extends BaseCell {
  type: 'simulation';
  iterations: number;
  seed?: number;
  dependencies: string[];
  output?: {
    results: number[];
    execution_time_ms: number;
    convergence: boolean;
  };
}

// ─── Markdown Cell ──────────────────────────────────────────────────

export interface MarkdownCell extends BaseCell {
  type: 'markdown';
  content: string;
  dependencies: []; // Markdown cells have no dependencies
}

// ─── Visualization Cell ─────────────────────────────────────────────

export type ChartType = 'histogram' | 'line' | 'scatter' | 'tornado' | 'calibration';

export interface VisualizationCell extends BaseCell {
  type: 'visualization';
  chart_type: ChartType;
  data_source: string; // Cell ID to visualize
  config: {
    title?: string;
    x_label?: string;
    y_label?: string;
    color?: string;
  };
  dependencies: string[];
}

// ─── Union Type for All Cells ───────────────────────────────────────

export type Cell =
  | QuestionCell
  | DriverCell
  | EvidenceCell
  | ModelCell
  | SimulationCell
  | MarkdownCell
  | VisualizationCell;

// ─── Notebook Document ──────────────────────────────────────────────

export interface Notebook {
  id: string;
  title: string;
  description?: string;
  permissions: NotebookPermissions;
  cells: Cell[];
  dependency_graph: DependencyGraph;
  metadata: {
    created_at: string;
    updated_at: string;
    version: number;
    author_id: string;
    tags: string[];
    portfolio_id?: string;
  };
}

// ─── Dependency Graph ───────────────────────────────────────────────

export interface DependencyGraph {
  nodes: string[]; // Cell IDs
  edges: DependencyEdge[];
}

export interface DependencyEdge {
  from: string; // Cell ID
  to: string;   // Cell ID
  type: 'data' | 'control';
}

// ─── Execution State ────────────────────────────────────────────────

export interface ExecutionState {
  status: 'idle' | 'running' | 'success' | 'error';
  current_cell?: string;
  completed_cells: string[];
  error_message?: string;
}

// ─── Agent Assist ───────────────────────────────────────────────────

export interface AgentSuggestion {
  agent_id: string;
  suggestion_type: 'new_cell' | 'edit_cell' | 'validation' | 'insight';
  target_cell_id?: string;
  content: {
    fpl_code?: string;
    reasoning: string;
    confidence: number;
  };
  actions: AgentAction[];
}

export interface AgentAction {
  type: 'insert_cell' | 'update_cell' | 'add_evidence' | 'refine_distribution';
  cell_id?: string;
  payload: any;
}

// ─── Brier Scoring ──────────────────────────────────────────────────

export interface BrierScore {
  forecast_id: string;
  question_cell_id: string;
  predicted_probability: number;
  actual_outcome: boolean;
  score: number; // (predicted - actual)^2
  resolved_at: string;
  resolution_source: string;
}

export interface CalibrationData {
  bins: CalibrationBin[];
  overall_brier: number;
  count: number;
}

export interface CalibrationBin {
  predicted_range: [number, number]; // e.g., [0.6, 0.7]
  actual_frequency: number;
  count: number;
}
