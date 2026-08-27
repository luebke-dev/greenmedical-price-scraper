import { fetchJson } from './client';
import type {
  History,
  HistoryBucket,
  Metadata,
  OfferHistoryMode,
  OfferHistoryPage,
  ReviewSort,
  ReviewsResponse,
  RunsResponse,
  RuleInput,
  RunStatus,
  StrainDetail,
  StrainsPage,
  Subscription,
  SubscriptionCreate,
  SubscriptionCreated,
} from './types';

/** Interactive OpenAPI documentation served by the backend (outside of /api/v1). */
export const API_DOCS_URL = '/api/docs';

export function getMetadata(signal?: AbortSignal): Promise<Metadata> {
  return fetchJson<Metadata>('/metadata', { signal });
}

export type StrainsSortKey =
  | 'price'
  | 'price_per_thc_gram'
  | 'thc'
  | 'cbd'
  | 'pharmacy_count'
  | 'rating'
  | 'name'
  | 'bezeichnung'
  | 'genetik';

export interface StrainsParams {
  q?: string | undefined;
  /** Lowercased genetik values; joined with commas. */
  genetik?: readonly string[] | undefined;
  price_min?: number | undefined;
  price_max?: number | undefined;
  thc_min?: number | undefined;
  thc_max?: number | undefined;
  cbd_min?: number | undefined;
  cbd_max?: number | undefined;
  rating_min?: number | undefined;
  sort?: StrainsSortKey | undefined;
  dir?: 'asc' | 'desc' | undefined;
  limit?: number | undefined;
  offset?: number | undefined;
}

export const STRAINS_DEFAULT_LIMIT = 50;

/** Numbers travel with at most two decimals ("5.4", "12.35"); `undefined` stays omitted. */
export function queryNumber(value: number | undefined): string | undefined {
  if (value === undefined || !Number.isFinite(value)) return undefined;
  return String(Math.round(value * 100) / 100);
}

/** Query for GET /strains; parameters at their defaults are omitted. */
export function strainsQuery(params: StrainsParams): Record<string, string | undefined> {
  const q = params.q?.trim();
  const genetik = params.genetik?.filter((item) => item !== '');
  return {
    q: q ? q : undefined,
    genetik: genetik && genetik.length > 0 ? genetik.join(',') : undefined,
    price_min: queryNumber(params.price_min),
    price_max: queryNumber(params.price_max),
    thc_min: queryNumber(params.thc_min),
    thc_max: queryNumber(params.thc_max),
    cbd_min: queryNumber(params.cbd_min),
    cbd_max: queryNumber(params.cbd_max),
    rating_min: queryNumber(params.rating_min),
    sort: params.sort && params.sort !== 'price' ? params.sort : undefined,
    dir: params.dir && params.dir !== 'asc' ? params.dir : undefined,
    limit:
      params.limit !== undefined && params.limit !== STRAINS_DEFAULT_LIMIT
        ? String(params.limit)
        : undefined,
    offset: params.offset ? String(params.offset) : undefined,
  };
}

export function getStrains(params: StrainsParams = {}, signal?: AbortSignal): Promise<StrainsPage> {
  return fetchJson<StrainsPage>('/strains', { query: strainsQuery(params), signal });
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

export interface OfferHistoryParams {
  from?: string | undefined;
  to?: string | undefined;
  bucket?: HistoryBucket | undefined;
  mode?: OfferHistoryMode | undefined;
  pharmacy_id?: number | undefined;
  limit?: number | undefined;
  offset?: number | undefined;
}

export function getOfferHistory(
  id: number,
  params: OfferHistoryParams = {},
  signal?: AbortSignal,
): Promise<OfferHistoryPage> {
  return fetchJson<OfferHistoryPage>(`/strains/${id}/offer-history`, {
    query: {
      from: params.from,
      to: params.to,
      bucket: params.bucket,
      mode: params.mode,
      pharmacy_id: params.pharmacy_id,
      limit: params.limit,
      offset: params.offset,
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

// --- Preisalarm-Abos --------------------------------------------------------

export function createSubscription(
  payload: SubscriptionCreate,
  signal?: AbortSignal,
): Promise<SubscriptionCreated> {
  return fetchJson<SubscriptionCreated>('/subscriptions', {
    method: 'POST',
    body: payload,
    signal,
  });
}

export function confirmSubscription(token: string, signal?: AbortSignal): Promise<Subscription> {
  return fetchJson<Subscription>('/subscriptions/confirm', {
    method: 'POST',
    body: { token },
    signal,
  });
}

export function getSubscription(token: string, signal?: AbortSignal): Promise<Subscription> {
  return fetchJson<Subscription>('/subscriptions/manage', { query: { token }, signal });
}

export function updateSubscription(
  token: string,
  rules: RuleInput[],
  signal?: AbortSignal,
): Promise<Subscription> {
  return fetchJson<Subscription>('/subscriptions/manage', {
    method: 'PUT',
    query: { token },
    body: { rules },
    signal,
  });
}

export function deleteSubscription(token: string, signal?: AbortSignal): Promise<void> {
  return fetchJson<void>('/subscriptions/manage', { method: 'DELETE', query: { token }, signal });
}
