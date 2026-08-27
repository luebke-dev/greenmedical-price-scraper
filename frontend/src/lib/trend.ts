import type { Trend } from '@/api/types';
import { de } from '@/i18n/de';
import { euro, signedEuro, signedPercent } from './format';

export type TrendDirection = Trend['direction'];

export const TREND_GLYPH: Readonly<Record<TrendDirection, string>> = {
  up: '▲', // ▲ price went up (amber)
  down: '▼', // ▼ price went down (green)
  flat: '–', // – unchanged (muted)
};

const DAY_MS = 86_400_000;

/** Whole days between the reference run and the latest run (at least 1). */
export function trendDays(trend: Trend, latestAt: string | null | undefined): number {
  const reference = new Date(trend.reference_at).valueOf();
  const latest = latestAt ? new Date(latestAt).valueOf() : Date.now();
  if (Number.isNaN(reference) || Number.isNaN(latest)) return 7;
  return Math.max(1, Math.round((latest - reference) / DAY_MS));
}

/** "vor 7 Tagen: 6,49 €/g (−0,50 €, −7,7 %)" */
export function trendTooltip(trend: Trend, latestAt: string | null | undefined): string {
  const ago = de.trend.ago(trendDays(trend, latestAt));
  const then = euro(trend.min_price_then, '€/g');
  return `${ago}: ${then} (${signedEuro(trend.delta)}, ${signedPercent(trend.delta_pct)})`;
}

export function trendLabel(direction: TrendDirection): string {
  return de.trend[direction];
}

/** Screen-reader text: "Preis gefallen, vor 7 Tagen: 6,49 €/g (−0,50 €, −7,7 %)" */
export function trendAriaLabel(trend: Trend, latestAt: string | null | undefined): string {
  return `${trendLabel(trend.direction)}, ${trendTooltip(trend, latestAt)}`;
}

export function trendClass(direction: TrendDirection): `trend-${TrendDirection}` {
  return `trend-${direction}`;
}
