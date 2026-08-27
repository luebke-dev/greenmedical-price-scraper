// Filter state ⇄ URL query (?q&genetics&preis&thc&cbd&sort&dir&page&size). Defaults are omitted.

import {
  RANGE_CONFIGS,
  clampRange,
  fullRanges,
  isFullRange,
  roundToStep,
  type BoundsState,
  type RangeBounds,
  type RangeKey,
  type RangeState,
  type RangeValue,
} from './filter';
import { DEFAULT_SORT, isDefaultSort, isSortDirection, isSortKey, type SortState } from './sort';

export const PAGE_SIZES: readonly number[] = [25, 50, 100];
export const DEFAULT_PAGE_SIZE = 50;

export interface FilterState {
  query: string;
  /** Lowercased genetics keys. */
  genetics: string[];
  ranges: RangeState;
  sort: SortState;
  /** 1-based page. */
  page: number;
  /** Rows per page, one of PAGE_SIZES. */
  size: number;
}

/** Page change requested by a table/pager (1-based page, rows per page). */
export interface PageRequest {
  page: number;
  size: number;
}

export function isPageSize(value: unknown): value is number {
  return typeof value === 'number' && PAGE_SIZES.includes(value);
}

/** Parses a positive integer ("3"); anything else → null. */
export function parsePositiveInt(raw: string | null): number | null {
  if (raw === null || !/^\d+$/.test(raw.trim())) return null;
  const value = Number(raw);
  return Number.isSafeInteger(value) && value >= 1 ? value : null;
}

/** Same value shape as vue-router's LocationQuery. */
export type QueryInput = Record<string, string | null | (string | null)[] | undefined>;
export type QueryOutput = Record<string, string | string[]>;

const RANGE_PARAM: Readonly<Record<RangeKey, string>> = {
  price: 'preis',
  thc: 'thc',
  cbd: 'cbd',
};

export function defaultFilterState(bounds: BoundsState = {}): FilterState {
  return {
    query: '',
    genetics: [],
    ranges: fullRanges(bounds),
    sort: { ...DEFAULT_SORT },
    page: 1,
    size: DEFAULT_PAGE_SIZE,
  };
}

export function formatRange(value: RangeValue, step: number): string {
  return `${roundToStep(value.lo, step)}-${roundToStep(value.hi, step)}`;
}

/**
 * Parses "5.4-12.3"; invalid input → null. With bounds the result is clamped into them; without
 * (facets not loaded yet) it is only normalised to lo <= hi and passed through as given.
 */
export function parseRange(raw: string, bounds?: RangeBounds): RangeValue | null {
  const match = /^\s*(-?\d+(?:\.\d+)?)\s*-\s*(-?\d+(?:\.\d+)?)\s*$/.exec(raw);
  if (!match) return null;
  const lo = Number(match[1]);
  const hi = Number(match[2]);
  if (!Number.isFinite(lo) || !Number.isFinite(hi)) return null;
  if (bounds) return clampRange({ lo, hi }, bounds);
  return { lo: Math.min(lo, hi), hi: Math.max(lo, hi) };
}

function first(value: string | null | (string | null)[] | undefined): string | null {
  if (Array.isArray(value)) {
    return value.find((item): item is string => typeof item === 'string') ?? null;
  }
  return typeof value === 'string' ? value : null;
}

function all(value: string | null | (string | null)[] | undefined): string[] {
  if (Array.isArray(value)) return value.filter((item): item is string => typeof item === 'string');
  return typeof value === 'string' ? [value] : [];
}

export interface FromQueryOptions {
  bounds: BoundsState;
  /** When given, unknown genetics keys are dropped. */
  geneticsKeys?: ReadonlySet<string> | undefined;
  /**
   * Keep ranges whose bounds are unknown (deep link before the facets arrived). They are sent to
   * the API as given and clamped once the facets are known.
   */
  passThroughRanges?: boolean | undefined;
}

export function fromQuery(query: QueryInput, options: FromQueryOptions): FilterState {
  const state = defaultFilterState(options.bounds);

  const q = first(query.q);
  if (q) state.query = q;

  const genetics = all(query.genetics)
    .map((item) => item.trim().toLowerCase())
    .filter((item) => item !== '' && (!options.geneticsKeys || options.geneticsKeys.has(item)));
  state.genetics = [...new Set(genetics)];

  for (const config of RANGE_CONFIGS) {
    const bounds = options.bounds[config.key];
    const raw = first(query[RANGE_PARAM[config.key]]);
    if (raw === null || (!bounds && !options.passThroughRanges)) continue;
    const parsed = parseRange(raw, bounds);
    if (parsed) state.ranges[config.key] = parsed;
  }

  const page = parsePositiveInt(first(query.page));
  if (page !== null) state.page = page;
  const size = parsePositiveInt(first(query.size));
  if (isPageSize(size)) state.size = size;

  const sort = first(query.sort);
  const dir = first(query.dir);
  if (isSortKey(sort)) {
    state.sort = { key: sort, direction: isSortDirection(dir) ? dir : 'asc' };
  }

  return state;
}

export function toQuery(state: FilterState, bounds: BoundsState): QueryOutput {
  const query: QueryOutput = {};
  const q = state.query.trim();
  if (q) query.q = q;
  if (state.genetics.length > 0) query.genetics = [...state.genetics];

  for (const config of RANGE_CONFIGS) {
    const range = bounds[config.key];
    const value = state.ranges[config.key];
    if (!value) continue;
    if (range) {
      if (isFullRange(value, range)) continue;
      query[RANGE_PARAM[config.key]] = formatRange(clampRange(value, range), config.step);
    } else {
      // Unknown bounds: keep the deep link as it is.
      query[RANGE_PARAM[config.key]] = formatRange(value, config.step);
    }
  }

  if (!isDefaultSort(state.sort)) {
    query.sort = state.sort.key;
    if (state.sort.direction !== 'asc') query.dir = state.sort.direction;
  }
  if (state.page > 1) query.page = String(state.page);
  if (state.size !== DEFAULT_PAGE_SIZE) query.size = String(state.size);
  return query;
}

/** Stable string form, used to detect "no change" without re-navigating. */
export function serializeQuery(query: QueryOutput): string {
  const params = new URLSearchParams();
  for (const key of Object.keys(query).sort()) {
    const value = query[key];
    if (Array.isArray(value)) {
      for (const item of value) params.append(key, item);
    } else if (value !== undefined) {
      params.append(key, value);
    }
  }
  return params.toString();
}
