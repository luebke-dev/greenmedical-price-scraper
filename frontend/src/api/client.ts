import type { ApiErrorBody } from './types';

export const API_BASE = '/api/v1';

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
  }
}

export type QueryValue = string | number | boolean | null | undefined;
export type QueryParams = Record<string, QueryValue>;

export interface FetchOptions {
  query?: QueryParams | undefined;
  signal?: AbortSignal | undefined;
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | undefined;
  /** Serialised as JSON (`Content-Type: application/json`). */
  body?: unknown;
}

/** Builds `${API_BASE}${path}?…` and omits null/undefined query values. */
export function buildUrl(path: string, query?: QueryParams): string {
  const params = new URLSearchParams();
  if (query) {
    for (const [key, value] of Object.entries(query)) {
      if (value === null || value === undefined) continue;
      params.set(key, String(value));
    }
  }
  const search = params.toString();
  return `${API_BASE}${path}${search ? `?${search}` : ''}`;
}

function isApiErrorBody(value: unknown): value is ApiErrorBody {
  if (typeof value !== 'object' || value === null) return false;
  const error = (value as { error?: unknown }).error;
  if (typeof error !== 'object' || error === null) return false;
  const { code, message } = error as { code?: unknown; message?: unknown };
  return typeof code === 'string' && typeof message === 'string';
}

async function toApiError(response: Response): Promise<ApiError> {
  let body: unknown = null;
  try {
    body = await response.json();
  } catch {
    // Non-JSON error body (e.g. from a proxy); fall through to the generic error.
  }
  if (isApiErrorBody(body)) {
    return new ApiError(response.status, body.error.code, body.error.message);
  }
  return new ApiError(response.status, 'http_error', `HTTP ${response.status}`);
}

export async function fetchJson<T>(path: string, options: FetchOptions = {}): Promise<T> {
  const headers: Record<string, string> = { Accept: 'application/json' };
  const init: RequestInit = { headers, method: options.method ?? 'GET' };
  if (options.signal) init.signal = options.signal;
  if (options.body !== undefined) {
    headers['Content-Type'] = 'application/json';
    init.body = JSON.stringify(options.body);
  }
  const response = await fetch(buildUrl(path, options.query), init);
  if (!response.ok) {
    throw await toApiError(response);
  }
  // 204 No Content (e.g. DELETE /subscriptions/manage).
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export function isAbortError(error: unknown): boolean {
  return error instanceof DOMException
    ? error.name === 'AbortError'
    : typeof error === 'object' &&
        error !== null &&
        (error as { name?: unknown }).name === 'AbortError';
}
