// de-DE formatting helpers, behaviourally identical to site/app.js.

export const collator = new Intl.Collator('de', { numeric: true, sensitivity: 'base' });

/** Typographic minus sign, used for signed deltas ("−0,50 €"). */
export const MINUS = '−';

const formatterCache = new Map<number, Intl.NumberFormat>();

function numberFormatter(decimals: number): Intl.NumberFormat {
  let formatter = formatterCache.get(decimals);
  if (!formatter) {
    formatter = new Intl.NumberFormat('de-DE', {
      minimumFractionDigits: decimals,
      maximumFractionDigits: decimals,
    });
    formatterCache.set(decimals, formatter);
  }
  return formatter;
}

function isMissing(value: number | null | undefined): value is null | undefined {
  return value === null || value === undefined || Number.isNaN(value);
}

/** Fixed-decimal number, e.g. num(5.4, 2) → "5,40". */
export function num(value: number, decimals: number): string {
  return numberFormatter(decimals).format(value);
}

/** Integer with thousands separators, e.g. 1234 → "1.234". */
export function integer(value: number): string {
  return value.toLocaleString('de-DE');
}

/** "5,49 €/g" – empty string for missing values (like the old formatEuro). */
export function euro(value: number | null | undefined, suffix: string): string {
  if (isMissing(value)) return '';
  return `${num(value, 2)} ${suffix}`;
}

/** "ab 5,49 €/g" – empty string for missing values. */
export function fromEuro(value: number | null | undefined, suffix: string): string {
  if (isMissing(value)) return '';
  return `ab ${euro(value, suffix)}`;
}

/** Signed euro delta with typographic minus: −0,50 € / +0,50 € / ±0,00 €. */
export function signedEuro(delta: number): string {
  return `${sign(delta)}${num(Math.abs(delta), 2)} €`;
}

/** Signed percentage with typographic minus: −7,7 % / +7,7 % / ±0,0 %. */
export function signedPercent(pct: number, decimals = 1): string {
  return `${sign(pct)}${num(Math.abs(pct), decimals)} %`;
}

function sign(value: number): string {
  if (value > 0) return '+';
  if (value < 0) return MINUS;
  return '±';
}

function parseDate(iso: string | null | undefined): Date | null {
  if (!iso) return null;
  const date = new Date(iso);
  return Number.isNaN(date.valueOf()) ? null : date;
}

/** "27.08.2026, 10:00" – empty string for invalid input. */
export function dateTime(iso: string | null | undefined): string {
  const date = parseDate(iso);
  return date ? date.toLocaleString('de-DE', { dateStyle: 'medium', timeStyle: 'short' }) : '';
}

/** "27.08.2026" – empty string for invalid input. */
export function dateOnly(iso: string | null | undefined): string {
  const date = parseDate(iso);
  return date ? date.toLocaleDateString('de-DE', { dateStyle: 'medium' }) : '';
}

/** "YYYY-MM-DD" (a Europe/Berlin calendar day from the API) → "27.08.2026". */
export function calendarDay(day: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
  if (!match) return day;
  return `${match[3]}.${match[2]}.${match[1]}`;
}

/** "4,3" – one decimal, e.g. for star ratings; empty string for missing values. */
export function rating(value: number | null | undefined): string {
  if (isMissing(value)) return '';
  return num(value, 1);
}

/** Compact rating "4,3 (124)" – empty string when there is no value or no reviews. */
export function ratingCompact(value: number | null | undefined, count: number | undefined): string {
  if (isMissing(value) || !count || count <= 0) return '';
  return `${rating(value)} (${integer(count)})`;
}
