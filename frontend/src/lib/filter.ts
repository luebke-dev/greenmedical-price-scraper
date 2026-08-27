import type { Strain } from '@/api/types';
import { de } from '@/i18n/de';
import { collator } from './format';
import { matchesSearch } from './search';

export type RangeKey = 'price' | 'thc' | 'cbd';

export interface RangeConfig {
  key: RangeKey;
  label: string;
  unit: string;
  step: number;
  decimals: number;
  get: (row: Strain) => number | null;
}

export const RANGE_CONFIGS: readonly RangeConfig[] = [
  {
    key: 'price',
    label: de.filters.price,
    unit: ' €/g',
    step: 0.1,
    decimals: 2,
    get: (row) => row.sort.price,
  },
  {
    key: 'thc',
    label: de.filters.thc,
    unit: ' %',
    step: 0.5,
    decimals: 1,
    get: (row) => row.sort.thc,
  },
  {
    key: 'cbd',
    label: de.filters.cbd,
    unit: ' %',
    step: 0.1,
    decimals: 1,
    get: (row) => row.sort.cbd,
  },
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
 * Slider bounds for one range filter: min floored / max ceiled to the step.
 * Returns null when there are no values or when min === max (nothing to filter).
 */
export function computeBounds(rows: readonly Strain[], config: RangeConfig): RangeBounds | null {
  let lowest = Number.POSITIVE_INFINITY;
  let highest = Number.NEGATIVE_INFINITY;
  for (const row of rows) {
    const value = config.get(row);
    if (value === null || value === undefined || Number.isNaN(value)) continue;
    if (value < lowest) lowest = value;
    if (value > highest) highest = value;
  }
  if (lowest === Number.POSITIVE_INFINITY) return null;
  const min = floorToStep(lowest, config.step);
  const max = ceilToStep(highest, config.step);
  if (min === max) return null;
  return { min, max };
}

export function computeAllBounds(rows: readonly Strain[]): BoundsState {
  const bounds: BoundsState = {};
  for (const config of RANGE_CONFIGS) {
    const range = computeBounds(rows, config);
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

/** Strains without a value only stay visible while the filter is untouched. */
export function matchesRange(row: Strain, ranges: RangeState, bounds: BoundsState): boolean {
  for (const config of RANGE_CONFIGS) {
    const range = bounds[config.key];
    const value = ranges[config.key];
    if (!range || !value) continue;
    const rowValue = config.get(row);
    const atFullRange = isFullRange(value, range);
    if (rowValue === null || rowValue === undefined) {
      if (!atFullRange) return false;
      continue;
    }
    if (rowValue < value.lo || rowValue > value.hi) return false;
  }
  return true;
}

export function genetikKey(label: string | null | undefined): string {
  return (label ?? '').toLowerCase();
}

export interface GenetikOption {
  key: string;
  label: string;
}

/**
 * Distinct genetics (case-insensitive), sorted by collator. Empty when fewer than 2.
 * Like site/app.js the label shows the casing of the LAST occurrence of a key.
 */
export function genetikOptions(rows: readonly Strain[]): GenetikOption[] {
  const byKey = new Map<string, string>();
  for (const row of rows) {
    const label = row.genetik || '';
    if (label) byKey.set(label.toLowerCase(), label);
  }
  if (byKey.size < 2) return [];
  return [...byKey.entries()]
    .map(([key, label]) => ({ key, label }))
    .sort((a, b) => collator.compare(a.label, b.label));
}

export function matchesGenetik(row: Strain, selected: ReadonlySet<string>): boolean {
  if (selected.size === 0) return true;
  return selected.has(genetikKey(row.genetik));
}

export interface FilterCriteria {
  query: string;
  genetik: ReadonlySet<string>;
  ranges: RangeState;
}

export function applyFilters(
  rows: readonly Strain[],
  criteria: FilterCriteria,
  bounds: BoundsState,
): Strain[] {
  return rows.filter(
    (row) =>
      matchesSearch(row, criteria.query) &&
      matchesRange(row, criteria.ranges, bounds) &&
      matchesGenetik(row, criteria.genetik),
  );
}
