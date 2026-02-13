/**
 * API Type Definitions
 *
 * Strict contracts between frontend and backend.
 * Ensures type safety across the network boundary.
 */

import type { Notebook, Cell, ExecutionState, AgentSuggestion, BrierScore, CalibrationData } from './notebook';

// ─── Notebook API ───────────────────────────────────────────────────

export interface CreateNotebookRequest {
  title: string;
  description?: string;
  visibility?: 'private' | 'shared' | 'public';
  team_id?: string;
  org_id?: string;
  template_id?: string;
}

export interface CreateNotebookResponse {
  notebook: Notebook;
}

export interface UpdateNotebookRequest {
  title?: string;
  description?: string;
  cells?: Cell[];
  visibility?: 'private' | 'shared' | 'public';
}

export interface UpdateNotebookResponse {
  notebook: Notebook;
  updated_cells: string[];
}

export interface ListNotebooksRequest {
  visibility?: 'private' | 'shared' | 'public';
  portfolio_id?: string;
  team_id?: string;
  org_id?: string;
  limit?: number;
  offset?: number;
}

export interface ListNotebooksResponse {
  notebooks: Notebook[];
  total: number;
}

// ─── Cell Execution API ─────────────────────────────────────────────

export interface ExecuteCellRequest {
  cell_id: string;
  dependencies: Record<string, any>; // Resolved values from dependent cells
}

export interface ExecuteCellResponse {
  output: any;
  updated_cells: string[]; // IDs of cells that need re-execution
  execution_time_ms: number;
}

export interface ExecuteNotebookRequest {
  iterations?: number;
  seed?: number;
}

export interface ExecuteNotebookResponse {
  cells: Record<string, any>; // cell_id -> output
  final_probability?: number;
  execution_state: ExecutionState;
  total_time_ms: number;
}

// ─── Agent Assist API ───────────────────────────────────────────────

export interface AgentAssistRequest {
  intent: string; // Natural language intent
  context: {
    current_cells: Cell[];
    active_cell_id?: string;
  };
  agent_id?: string; // Default: 'fermi_coach'
}

export interface AgentAssistResponse {
  suggestions: AgentSuggestion[];
  agent_reasoning: string;
}

// ─── Validation API ─────────────────────────────────────────────────

export interface ValidateFPLRequest {
  fpl_code: string;
  cell_type: Cell['type'];
}

export interface ValidateFPLResponse {
  valid: boolean;
  errors: ValidationError[];
  warnings: ValidationWarning[];
  ast?: any;
}

export interface ValidationError {
  line: number;
  column: number;
  message: string;
  severity: 'error';
}

export interface ValidationWarning {
  line: number;
  column: number;
  message: string;
  severity: 'warning';
}

// ─── Brier Scoring API ──────────────────────────────────────────────

export interface ResolveForec astRequest {
  notebook_id: string;
  question_cell_id: string;
  actual_outcome: boolean;
  resolution_source: string;
}

export interface ResolveForecastResponse {
  brier_score: BrierScore;
}

export interface GetCalibrationRequest {
  user_id?: string;
  team_id?: string;
  org_id?: string;
  portfolio_id?: string;
  time_range?: {
    start: string;
    end: string;
  };
}

export interface GetCalibrationResponse {
  calibration: CalibrationData;
}

// ─── Portfolio API ──────────────────────────────────────────────────

export interface GetPortfolioRequest {
  portfolio_id: string;
}

export interface GetPortfolioResponse {
  forecasts: PortfolioForecast[];
  calibration: CalibrationData;
  sensitivity_matrix: number[][];
}

export interface PortfolioForecast {
  id: string;
  notebook_id: string;
  question: string;
  probability: number;
  last_updated: string;
  brier_score?: number;
  sparkline_data: number[]; // Historical probability values
}
