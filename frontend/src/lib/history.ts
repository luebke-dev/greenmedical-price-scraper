import type { History, HistoryBucket, HistoryPoint, PharmacySeriesPoint } from '@/api/types';
import { de } from '@/i18n/de';
import { calendarDay, dateTime, euro } from './format';

export type HistoryPreset = '7d' | '30d' | '90d' | 'all';

export const HISTORY_PRESETS: readonly HistoryPreset[] = ['7d', '30d', '90d', 'all'];

export const DEFAULT_PRESET: HistoryPreset = '30d';

/** The backend rejects spans above 730 days. */
export const MAX_HISTORY_DAYS = 730;

/**
 * "Alles" stays one day inside the limit: an inclusive check or a rounding difference on the
 * backend must never turn the preset into a 400.
 */
export const ALL_PRESET_DAYS = MAX_HISTORY_DAYS - 1;

const DAY_MS = 86_400_000;

export interface HistoryRange {
  from: string;
  to: string;
  bucket: HistoryBucket;
}

export function isHistoryPreset(value: unknown): value is HistoryPreset {
  return typeof value === 'string' && (HISTORY_PRESETS as readonly string[]).includes(value);
}

export function presetDays(preset: HistoryPreset): number {
  switch (preset) {
    case '7d':
      return 7;
    case '30d':
      return 30;
    case '90d':
      return 90;
    case 'all':
      return ALL_PRESET_DAYS;
  }
}

/** Short spans show every run; long spans are bucketed per Berlin day. */
export function presetBucket(preset: HistoryPreset): HistoryBucket {
  return preset === '7d' || preset === '30d' ? 'run' : 'day';
}

/** Maps a preset to API parameters. `to` is truncated to the minute so cache keys stay stable. */
export function presetRange(preset: HistoryPreset, now: Date = new Date()): HistoryRange {
  const to = new Date(Math.floor(now.valueOf() / 60_000) * 60_000);
  const from = new Date(to.valueOf() - presetDays(preset) * DAY_MS);
  return { from: from.toISOString(), to: to.toISOString(), bucket: presetBucket(preset) };
}

export function formatHistoryAt(at: string, bucket: HistoryBucket): string {
  return bucket === 'day' ? calendarDay(at) : dateTime(at);
}

export interface PharmacyLine {
  id: number;
  name: string;
  city: string;
  data: (number | null)[];
}

export interface HistorySeries {
  unit: string;
  bucket: HistoryBucket;
  /** Original `at` values (x axis keys). */
  keys: string[];
  /** Formatted x axis labels. */
  categories: string[];
  min: (number | null)[];
  avg: (number | null)[];
  max: (number | null)[];
  /** Stacked band: lower edge (= min) and width (= max − min). */
  bandLower: (number | null)[];
  bandWidth: (number | null)[];
  offerCount: number[];
  pharmacyCount: number[];
  pharmacies: PharmacyLine[];
}

export interface SeriesOptions {
  thcMode: boolean;
  pharmacies: boolean;
}

export const UNIT_GRAM = '€/g';
export const UNIT_THC = '€/g THC';

function pointValues(point: HistoryPoint, thcMode: boolean) {
  return thcMode
    ? { min: point.min_per_thc_gram, avg: point.avg_per_thc_gram, max: point.max_per_thc_gram }
    : { min: point.min, avg: point.avg, max: point.max };
}

function pharmacyValue(point: PharmacySeriesPoint, thcMode: boolean): number | null {
  return thcMode ? point.price_per_thc_gram : point.price;
}

function round2(value: number): number {
  return Math.round(value * 100) / 100;
}

/** Turns an API history into chart-ready series; pharmacy lines keep gaps as null. */
export function buildSeries(history: History, options: SeriesOptions): HistorySeries {
  const keys = history.points.map((point) => point.at);
  const indexByKey = new Map<string, number>(keys.map((key, index) => [key, index]));
  const series: HistorySeries = {
    unit: options.thcMode ? UNIT_THC : UNIT_GRAM,
    bucket: history.bucket,
    keys,
    categories: keys.map((key) => formatHistoryAt(key, history.bucket)),
    min: [],
    avg: [],
    max: [],
    bandLower: [],
    bandWidth: [],
    offerCount: [],
    pharmacyCount: [],
    pharmacies: [],
  };

  for (const point of history.points) {
    const { min, avg, max } = pointValues(point, options.thcMode);
    series.min.push(min);
    series.avg.push(avg);
    series.max.push(max);
    series.bandLower.push(min);
    series.bandWidth.push(min !== null && max !== null ? round2(max - min) : null);
    series.offerCount.push(point.offer_count);
    series.pharmacyCount.push(point.pharmacy_count);
  }

  if (options.pharmacies && history.pharmacies) {
    for (const pharmacy of history.pharmacies) {
      const data: (number | null)[] = new Array<number | null>(keys.length).fill(null);
      for (const point of pharmacy.points) {
        const index = indexByKey.get(point.at);
        if (index === undefined) continue;
        data[index] = pharmacyValue(point, options.thcMode);
      }
      series.pharmacies.push({
        id: pharmacy.pharmacy_id,
        name: pharmacy.name,
        city: pharmacy.city,
        data,
      });
    }
  }

  return series;
}

export interface HistoryTableRow {
  at: string;
  min: number | null;
  avg: number | null;
  max: number | null;
  offerCount: number;
  pharmacyCount: number;
}

/** Rows for the "Daten als Tabelle" fallback. */
export function historyTableRows(history: History, thcMode: boolean): HistoryTableRow[] {
  return history.points.map((point) => ({
    at: formatHistoryAt(point.at, history.bucket),
    ...pointValues(point, thcMode),
    offerCount: point.offer_count,
    pharmacyCount: point.pharmacy_count,
  }));
}

export function seriesAriaLabel(name: string, series: HistorySeries): string {
  const first = series.categories[0] ?? '';
  const last = series.categories[series.categories.length - 1] ?? '';
  return de.history.chartAria(name, series.unit, series.categories.length, first, last);
}

export interface OfferHistoryRow {
  key: string;
  date: string;
  /** True for the first row of a run/day group (used for visual separation). */
  first: boolean;
  pharmacy: string;
  city: string;
  price: string;
  thcPrice: string;
  availability: string;
}

/**
 * Flattens the per-pharmacy series of a history response into one row per (run, pharmacy),
 * newest run first, pharmacies alphabetically within a run. Requires `pharmacies=true`.
 */
export function offerHistoryRows(history: History): OfferHistoryRow[] {
  const collator = new Intl.Collator('de', { sensitivity: 'base' });
  const flat: (Omit<OfferHistoryRow, 'first'> & { at: string })[] = [];
  for (const series of history.pharmacies ?? []) {
    for (const point of series.points) {
      if (point.price === null && !point.availability) continue;
      flat.push({
        key: `${point.at}|${series.pharmacy_id}`,
        at: point.at,
        date: formatHistoryAt(point.at, history.bucket),
        pharmacy: series.name,
        city: series.city,
        price: euro(point.price, '€/g'),
        thcPrice: euro(point.price_per_thc_gram, '€/g THC'),
        availability: point.availability,
      });
    }
  }
  flat.sort((a, b) =>
    a.at < b.at ? 1 : a.at > b.at ? -1 : collator.compare(a.pharmacy, b.pharmacy),
  );
  return flat.map((row, index) => ({
    ...row,
    first: index === 0 || flat[index - 1]!.at !== row.at,
  }));
}

export interface OfferPhaseRow {
  key: string;
  pharmacy: string;
  city: string;
  price: string;
  thcPrice: string;
  availability: string;
  /** Formatted start of the phase (first run with this price/status). */
  from: string;
  /** Formatted end (last run seen with this state); null when it still holds in the latest run. */
  to: string | null;
  /** Number of runs/days the phase covers. */
  runs: number;
  /** True when the pharmacy stopped listing the strain in this phase. */
  delisted: boolean;
  rawFrom: string;
}

/**
 * Condenses the per-pharmacy series into phases: one row per pharmacy and consecutive stretch of
 * runs with the same price + status. A pharmacy missing from a run (while the strain itself was
 * seen in that run) starts a "nicht mehr gelistet" phase. Newest phase first.
 */
export function offerHistoryPhases(history: History): OfferPhaseRow[] {
  const collator = new Intl.Collator('de', { sensitivity: 'base' });
  const runs = [...new Set(history.points.map((point) => point.at))].sort();
  if (runs.length === 0) return [];
  const latest = runs[runs.length - 1]!;
  const rows: OfferPhaseRow[] = [];

  for (const series of history.pharmacies ?? []) {
    const byAt = new Map(series.points.map((point) => [point.at, point]));
    let current: (OfferPhaseRow & { stateKey: string }) | null = null;
    let seen = false;
    for (const at of runs) {
      const point = byAt.get(at);
      const listed = point !== undefined && (point.price !== null || point.availability !== '');
      if (!listed && !seen) continue; // ignore runs before the pharmacy first listed the strain
      seen = true;
      const stateKey = listed ? `${point.price ?? ''}|${point.availability}` : 'delisted';
      if (current && current.stateKey === stateKey) {
        current.to = at === latest ? null : formatHistoryAt(at, history.bucket);
        current.runs += 1;
        continue;
      }
      if (current) rows.push(stripState(current));
      current = {
        stateKey,
        key: `${series.pharmacy_id}|${at}`,
        pharmacy: series.name,
        city: series.city,
        price: listed ? euro(point.price, '€/g') : '',
        thcPrice: listed ? euro(point.price_per_thc_gram, '€/g THC') : '',
        availability: listed ? point.availability : '',
        from: formatHistoryAt(at, history.bucket),
        to: at === latest ? null : formatHistoryAt(at, history.bucket),
        runs: 1,
        delisted: !listed,
        rawFrom: at,
      };
    }
    if (current) rows.push(stripState(current));
  }

  rows.sort((a, b) =>
    a.rawFrom < b.rawFrom
      ? 1
      : a.rawFrom > b.rawFrom
        ? -1
        : collator.compare(a.pharmacy, b.pharmacy),
  );
  return rows;
}

function stripState(row: OfferPhaseRow & { stateKey: string }): OfferPhaseRow {
  const { stateKey: _omit, ...rest } = row;
  void _omit;
  return rest;
}
