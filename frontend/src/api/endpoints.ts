import { API_BASE, fetchJson } from './client';
import type {
  History,
  HistoryBucket,
  Metadata,
  ReviewSort,
  ReviewsResponse,
  RunsResponse,
  RunStatus,
  StrainDetail,
  StrainsResponse,
} from './types';

export const EXPORT_CSV_URL = `${API_BASE}/export.csv`;
export const EXPORT_JSON_URL = `${API_BASE}/export.json`;

export function getMetadata(signal?: AbortSignal): Promise<Metadata> {
  return fetchJson<Metadata>('/metadata', { signal });
}

export function getStrains(signal?: AbortSignal): Promise<StrainsResponse> {
  return fetchJson<StrainsResponse>('/strains', { signal });
}

export function getStrain(id: number, signal?: AbortSignal): Promise<StrainDetail> {
  return fetchJson<StrainDetail>(`/strains/${id}`, { signal });
}

export interface HistoryParams {
  from?: string | undefined;
  to?: string | undefined;
  bucket?: HistoryBucket | undefined;
  includePartial?: boolean | undefined;
  pharmacies?: boolean | undefined;
}

export function getStrainHistory(
  id: number,
  params: HistoryParams = {},
  signal?: AbortSignal,
): Promise<History> {
  return fetchJson<History>(`/strains/${id}/history`, {
    query: {
      from: params.from,
      to: params.to,
      bucket: params.bucket,
      include_partial: params.includePartial,
      pharmacies: params.pharmacies,
    },
    signal,
  });
}

export interface RunsParams {
  limit?: number | undefined;
  offset?: number | undefined;
  status?: RunStatus | undefined;
}

export function getRuns(params: RunsParams = {}, signal?: AbortSignal): Promise<RunsResponse> {
  return fetchJson<RunsResponse>('/runs', {
    query: { limit: params.limit, offset: params.offset, status: params.status },
    signal,
  });
}

export interface ReviewsParams {
  limit?: number | undefined;
  offset?: number | undefined;
  sort?: ReviewSort | undefined;
}

export function getReviews(
  id: number,
  params: ReviewsParams = {},
  signal?: AbortSignal,
): Promise<ReviewsResponse> {
  return fetchJson<ReviewsResponse>(`/strains/${id}/reviews`, {
    query: { limit: params.limit, offset: params.offset, sort: params.sort },
    signal,
  });
}
