import type { OfferHistoryParams } from '@/api/endpoints';
import type {
  History,
  HistoryBucket,
  HistoryPoint,
  OfferHistoryMode,
  PharmacySeriesPoint,
} from '@/api/types';
import { de } from '@/i18n/de';
import { calendarDay, dateTime } from './format';

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

export const OFFER_HISTORY_SIZES: readonly number[] = [25, 50, 100];
export const DEFAULT_OFFER_HISTORY_SIZE = 50;
export const DEFAULT_OFFER_HISTORY_MODE: OfferHistoryMode = 'changes';

export interface OfferHistoryQueryState {
  mode: OfferHistoryMode;
  /** 1-based page. */
  page: number;
  size: number;
}

/** Range + mode + page → GET /strains/{id}/offer-history parameters (limit/offset). */
export function buildOfferHistoryParams(
  range: HistoryRange,
  state: OfferHistoryQueryState,
): OfferHistoryParams {
  return {
    from: range.from,
    to: range.to,
    bucket: range.bucket,
    mode: state.mode,
    limit: state.size,
    offset: Math.max(0, state.page - 1) * state.size,
  };
}
