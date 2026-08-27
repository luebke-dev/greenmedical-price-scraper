// Range/genetics filter helpers and the GET /strains query builder. Filtering and sorting happen
// on the server; this module only shapes the state into parameters and the facets into bounds.

import type { StrainsParams } from '@/api/endpoints';
import type { FacetRange, Facets } from '@/api/types';
import { de } from '@/i18n/de';
import type { SortState } from './sort';

export type RangeKey = 'price' | 'thc' | 'cbd';

export interface RangeConfig {
  key: RangeKey;
  label: string;
  unit: string;
  step: number;
  decimals: number;
}

export const RANGE_CONFIGS: readonly RangeConfig[] = [
  { key: 'price', label: de.filters.price, unit: ' €/g', step: 0.1, decimals: 2 },
  { key: 'thc', label: de.filters.thc, unit: ' %', step: 0.5, decimals: 1 },
  { key: 'cbd', label: de.filters.cbd, unit: ' %', step: 0.1, decimals: 1 },
];

export function rangeConfig(key: RangeKey): RangeConfig {
  const config = RANGE_CONFIGS.find((item) => item.key === key);
  if (!config) throw new Error(`unknown range key: ${key}`);
  return config;
}

export interface RangeBounds {
  min: number;
  max: number;
}

export interface RangeValue {
  lo: number;
  hi: number;
}

export type BoundsState = Partial<Record<RangeKey, RangeBounds>>;
export type RangeState = Partial<Record<RangeKey, RangeValue>>;

/** Number of decimals needed to represent `step` exactly (0.5 → 1, 0.1 → 1). */
export function stepDecimals(step: number): number {
  const text = String(step);
  const index = text.indexOf('.');
  return index === -1 ? 0 : text.length - index - 1;
}

/** Rounds away floating point noise, e.g. 5.4000000000000004 → 5.4. */
export function roundToStep(value: number, step: number): number {
  return Number(value.toFixed(stepDecimals(step)));
}

const EPSILON = 1e-9;

/*
 * Deliberate deviation from site/app.js, which used plain `Math.floor(min / step) * step`:
 * floating point noise made exact multiples of the step drift one step outwards there
 * (e.g. a CBD minimum of 0.3 → `0.3 / 0.1 = 2.9999999999999996` → slider minimum 0,20).
 * Here exact multiples stay put and the result is rounded to the step's decimals, so the slider
 * bounds always equal the displayed values.
 */
export function floorToStep(value: number, step: number): number {
  return roundToStep(Math.floor(value / step + EPSILON) * step, step);
}

export function ceilToStep(value: number, step: number): number {
  return roundToStep(Math.ceil(value / step - EPSILON) * step, step);
}

/**
 * Slider bounds from a raw facet range: min floored / max ceiled to the step.
 * Returns null without a facet or when min === max (nothing to filter).
 */
export function boundsFromFacet(facet: FacetRange | null, config: RangeConfig): RangeBounds | null {
  if (!facet || !Number.isFinite(facet.min) || !Number.isFinite(facet.max)) return null;
  const min = floorToStep(facet.min, config.step);
  const max = ceilToStep(facet.max, config.step);
  if (min >= max) return null;
  return { min, max };
}

export function boundsFromFacets(facets: Facets | null | undefined): BoundsState {
  const bounds: BoundsState = {};
  if (!facets) return bounds;
  for (const config of RANGE_CONFIGS) {
    const range = boundsFromFacet(facets[config.key], config);
    if (range) bounds[config.key] = range;
  }
  return bounds;
}

export function fullRange(bounds: RangeBounds): RangeValue {
  return { lo: bounds.min, hi: bounds.max };
}

export function fullRanges(bounds: BoundsState): RangeState {
  const ranges: RangeState = {};
  for (const config of RANGE_CONFIGS) {
    const range = bounds[config.key];
    if (range) ranges[config.key] = fullRange(range);
  }
  return ranges;
}

export function isFullRange(value: RangeValue, bounds: RangeBounds): boolean {
  return value.lo <= bounds.min && value.hi >= bounds.max;
}

/** Clamps into the bounds and makes sure lo <= hi. */
export function clampRange(value: RangeValue, bounds: RangeBounds): RangeValue {
  const clamp = (n: number) => Math.min(bounds.max, Math.max(bounds.min, n));
  const lo = clamp(Math.min(value.lo, value.hi));
  const hi = clamp(Math.max(value.lo, value.hi));
  return { lo, hi };
}

export function geneticsKey(label: string | null | undefined): string {
  return (label ?? '').toLowerCase();
}

export interface GeneticsOption {
  key: string;
  label: string;
  count?: number | undefined;
}

/** Chip options from the facets (already alphabetical, de). Empty when fewer than 2. */
export function geneticsFromFacets(facets: Facets | null | undefined): GeneticsOption[] {
  const options: GeneticsOption[] = [];
  const seen = new Set<string>();
  for (const item of facets?.genetics ?? []) {
    const key = geneticsKey(item.value);
    if (key === '' || seen.has(key)) continue;
    seen.add(key);
    options.push({ key, label: item.value, count: item.count });
  }
  return options.length < 2 ? [] : options;
}

export interface StrainsQueryState {
  query: string;
  /** Lowercased genetics keys. */
  genetics: readonly string[];
  ranges: RangeState;
  sort: SortState;
  /** 1-based page. */
  page: number;
  size: number;
}

/**
 * Filter state → GET /strains parameters. A slider at its full width (or a range whose bounds
 * are unknown yet but that is not narrowed) is omitted; without bounds (facets not loaded yet,
 * e.g. a cold-start deep link) the range is sent as given.
 */
export function buildStrainsParams(state: StrainsQueryState, bounds: BoundsState): StrainsParams {
  const params: StrainsParams = {};
  const q = state.query.trim();
  if (q) params.q = q;
  if (state.genetics.length > 0) params.genetics = [...state.genetics];

  for (const config of RANGE_CONFIGS) {
    const value = state.ranges[config.key];
    if (!value) continue;
    const range = bounds[config.key];
    if (range && isFullRange(value, range)) continue;
    const clamped = range ? clampRange(value, range) : value;
    if (!range || clamped.lo > range.min) params[`${config.key}_min`] = clamped.lo;
    if (!range || clamped.hi < range.max) params[`${config.key}_max`] = clamped.hi;
  }

  params.sort = state.sort.key;
  params.dir = state.sort.direction;
  params.limit = state.size;
  params.offset = Math.max(0, state.page - 1) * state.size;
  return params;
}
