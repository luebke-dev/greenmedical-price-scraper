// Filter state ⇄ URL query (?q&genetik&preis&thc&cbd&sort&dir). Defaults are omitted.

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

export interface FilterState {
  query: string;
  /** Lowercased genetik keys. */
  genetik: string[];
  ranges: RangeState;
  sort: SortState;
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
  return { query: '', genetik: [], ranges: fullRanges(bounds), sort: { ...DEFAULT_SORT } };
}

export function formatRange(value: RangeValue, step: number): string {
  return `${roundToStep(value.lo, step)}-${roundToStep(value.hi, step)}`;
}

/** Parses "5.4-12.3"; invalid input → null. The result is clamped into the bounds. */
export function parseRange(raw: string, bounds: RangeBounds): RangeValue | null {
  const match = /^\s*(-?\d+(?:\.\d+)?)\s*-\s*(-?\d+(?:\.\d+)?)\s*$/.exec(raw);
  if (!match) return null;
  const lo = Number(match[1]);
  const hi = Number(match[2]);
  if (!Number.isFinite(lo) || !Number.isFinite(hi)) return null;
  return clampRange({ lo, hi }, bounds);
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
  /** When given, unknown genetik keys are dropped. */
  genetikKeys?: ReadonlySet<string> | undefined;
}

export function fromQuery(query: QueryInput, options: FromQueryOptions): FilterState {
  const state = defaultFilterState(options.bounds);

  const q = first(query.q);
  if (q) state.query = q;

  const genetik = all(query.genetik)
    .map((item) => item.trim().toLowerCase())
    .filter((item) => item !== '' && (!options.genetikKeys || options.genetikKeys.has(item)));
  state.genetik = [...new Set(genetik)];

  for (const config of RANGE_CONFIGS) {
    const bounds = options.bounds[config.key];
    const raw = first(query[RANGE_PARAM[config.key]]);
    if (!bounds || raw === null) continue;
    const parsed = parseRange(raw, bounds);
    if (parsed) state.ranges[config.key] = parsed;
  }

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
  if (state.genetik.length > 0) query.genetik = [...state.genetik];

  for (const config of RANGE_CONFIGS) {
    const range = bounds[config.key];
    const value = state.ranges[config.key];
    if (!range || !value || isFullRange(value, range)) continue;
    query[RANGE_PARAM[config.key]] = formatRange(clampRange(value, range), config.step);
  }

  if (!isDefaultSort(state.sort)) {
    query.sort = state.sort.key;
    if (state.sort.direction !== 'asc') query.dir = state.sort.direction;
  }
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
