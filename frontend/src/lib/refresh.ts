import { de } from '@/i18n/de';

export const MINUTE_MS = 60_000;
export const HOUR_MS = 60 * MINUTE_MS;

/** Parses `next_run_at`; NaN for garbage. */
export function nextRunTime(nextRunAt: string | null | undefined): number {
  return nextRunAt ? Date.parse(nextRunAt) : Number.NaN;
}

/**
 * Countdown text for the banner: "… in weniger als einer Minute", "… in 1 Minute",
 * "… in 37 Minuten", "… in 2 Std.", "… in 1 Std. 5 Min.". Remaining time is rounded up to
 * whole minutes, so the text never promises a run that is already due.
 */
export function countdownText(nextRunAt: string, now: number): string {
  const remaining = nextRunTime(nextRunAt) - now;
  if (!Number.isFinite(remaining) || remaining < MINUTE_MS) return de.refresh.lessThanMinute;
  const minutes = Math.ceil(remaining / MINUTE_MS);
  if (minutes < 60) {
    return minutes === 1 ? de.refresh.minute : de.refresh.minutes.replace('{n}', String(minutes));
  }
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (rest === 0) return de.refresh.hours.replace('{h}', String(hours));
  return de.refresh.hoursMinutes.replace('{h}', String(hours)).replace('{m}', String(rest));
}

/** Milliseconds until the next full wall-clock minute (at least 1 ms). */
export function msUntilNextMinute(now: number): number {
  const rest = MINUTE_MS - (now % MINUTE_MS);
  return rest <= 0 ? MINUTE_MS : rest;
}
