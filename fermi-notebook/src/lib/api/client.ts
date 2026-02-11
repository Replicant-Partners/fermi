/**
 * Fermi API Client
 *
 * Type-safe HTTP client for all backend interactions.
 * Single source of truth for API communication.
 */

import type {
  CreateNotebookRequest,
  CreateNotebookResponse,
  UpdateNotebookRequest,
  UpdateNotebookResponse,
  ListNotebooksRequest,
  ListNotebooksResponse,
  ExecuteCellRequest,
  ExecuteCellResponse,
  ExecuteNotebookRequest,
  ExecuteNotebookResponse,
  AgentAssistRequest,
  AgentAssistResponse,
  ValidateFPLRequest,
  ValidateFPLResponse,
  ResolveForecastRequest,
  ResolveForecastResponse,
  GetCalibrationRequest,
  GetCalibrationResponse,
  GetPortfolioRequest,
  GetPortfolioResponse,
} from '../types/api';
import type { Notebook } from '../types/notebook';

// ─── Configuration ──────────────────────────────────────────────────

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL || '/api';

// ─── HTTP Utilities ─────────────────────────────────────────────────

async function fetchJSON<T>(url: string, options?: RequestInit): Promise<T> {
  const response = await fetch(url, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...options?.headers,
    },
  });

  if (!response.ok) {
    const error = await response.text();
    throw new Error(`API Error ${response.status}: ${error}`);
  }

  return response.json();
}

// ─── Notebook API ───────────────────────────────────────────────────

export async function createNotebook(
  req: CreateNotebookRequest
): Promise<CreateNotebookResponse> {
  return fetchJSON(`${API_BASE_URL}/notebooks`, {
    method: 'POST',
    body: JSON.stringify(req),
  });
}

export async function getNotebook(id: string): Promise<Notebook> {
  return fetchJSON(`${API_BASE_URL}/notebooks/${id}`);
}

export async function updateNotebook(
  id: string,
  req: UpdateNotebookRequest
): Promise<UpdateNotebookResponse> {
  return fetchJSON(`${API_BASE_URL}/notebooks/${id}`, {
    method: 'PUT',
    body: JSON.stringify(req),
  });
}

export async function deleteNotebook(id: string): Promise<void> {
  return fetchJSON(`${API_BASE_URL}/notebooks/${id}`, {
    method: 'DELETE',
  });
}

export async function listNotebooks(
  req: ListNotebooksRequest = {}
): Promise<ListNotebooksResponse> {
  const params = new URLSearchParams();
  if (req.visibility) params.set('visibility', req.visibility);
  if (req.portfolio_id) params.set('portfolio_id', req.portfolio_id);
  if (req.team_id) params.set('team_id', req.team_id);
  if (req.org_id) params.set('org_id', req.org_id);
  if (req.limit) params.set('limit', req.limit.toString());
  if (req.offset) params.set('offset', req.offset.toString());

  return fetchJSON(`${API_BASE_URL}/notebooks?${params.toString()}`);
}

// ─── Execution API ──────────────────────────────────────────────────

export async function executeCell(
  notebookId: string,
  cellId: string,
  req: ExecuteCellRequest
): Promise<ExecuteCellResponse> {
  return fetchJSON(`${API_BASE_URL}/notebooks/${notebookId}/cells/${cellId}/execute`, {
    method: 'POST',
    body: JSON.stringify(req),
  });
}

export async function executeNotebook(
  notebookId: string,
  req: ExecuteNotebookRequest = {}
): Promise<ExecuteNotebookResponse> {
  return fetchJSON(`${API_BASE_URL}/notebooks/${notebookId}/execute`, {
    method: 'POST',
    body: JSON.stringify(req),
  });
}

// ─── Agent Assist API ───────────────────────────────────────────────

export async function getAgentAssist(
  notebookId: string,
  req: AgentAssistRequest
): Promise<AgentAssistResponse> {
  return fetchJSON(`${API_BASE_URL}/notebooks/${notebookId}/assist`, {
    method: 'POST',
    body: JSON.stringify(req),
  });
}

// ─── Validation API ─────────────────────────────────────────────────

export async function validateFPL(
  req: ValidateFPLRequest
): Promise<ValidateFPLResponse> {
  return fetchJSON(`${API_BASE_URL}/fpl/validate`, {
    method: 'POST',
    body: JSON.stringify(req),
  });
}

// ─── Brier Scoring API ──────────────────────────────────────────────

export async function resolveForecast(
  req: ResolveForecastRequest
): Promise<ResolveForecastResponse> {
  return fetchJSON(`${API_BASE_URL}/forecasts/resolve`, {
    method: 'POST',
    body: JSON.stringify(req),
  });
}

export async function getCalibration(
  req: GetCalibrationRequest = {}
): Promise<GetCalibrationResponse> {
  const params = new URLSearchParams();
  if (req.user_id) params.set('user_id', req.user_id);
  if (req.team_id) params.set('team_id', req.team_id);
  if (req.org_id) params.set('org_id', req.org_id);
  if (req.portfolio_id) params.set('portfolio_id', req.portfolio_id);
  if (req.time_range) {
    params.set('start', req.time_range.start);
    params.set('end', req.time_range.end);
  }

  return fetchJSON(`${API_BASE_URL}/calibration?${params.toString()}`);
}

// ─── Portfolio API ──────────────────────────────────────────────────

export async function getPortfolio(
  req: GetPortfolioRequest
): Promise<GetPortfolioResponse> {
  return fetchJSON(`${API_BASE_URL}/portfolios/${req.portfolio_id}`);
}
