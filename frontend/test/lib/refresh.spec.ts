import { describe, expect, it } from 'vitest';
import { countdownText, msUntilNextMinute, nextRunTime } from '@/lib/refresh';

const NOW = Date.parse('2026-08-27T20:00:00Z');
const at = (offsetMs: number) => new Date(NOW + offsetMs).toISOString();

describe('countdownText', () => {
  it('says "weniger als einer Minute" below one minute, in the past and for garbage', () => {
    expect(countdownText(at(30_000), NOW)).toBe(
      'Nächste Aktualisierung in weniger als einer Minute',
    );
    expect(countdownText(at(59_999), NOW)).toContain('weniger als einer Minute');
    expect(countdownText(at(-5_000), NOW)).toContain('weniger als einer Minute');
    expect(countdownText('kaputt', NOW)).toContain('weniger als einer Minute');
  });

  it('uses the singular for exactly one minute', () => {
    expect(countdownText(at(60_000), NOW)).toBe('Nächste Aktualisierung in 1 Minute');
  });

  it('rounds up to whole minutes', () => {
    expect(countdownText(at(37 * 60_000), NOW)).toBe('Nächste Aktualisierung in 37 Minuten');
    expect(countdownText(at(36 * 60_000 + 1), NOW)).toBe('Nächste Aktualisierung in 37 Minuten');
  });

  it('formats hours with and without a minute remainder', () => {
    expect(countdownText(at(65 * 60_000), NOW)).toBe('Nächste Aktualisierung in 1 Std. 5 Min.');
    expect(countdownText(at(120 * 60_000), NOW)).toBe('Nächste Aktualisierung in 2 Std.');
    expect(countdownText(at(60 * 60_000), NOW)).toBe('Nächste Aktualisierung in 1 Std.');
  });
});

describe('nextRunTime', () => {
  it('parses RFC 3339 and yields NaN otherwise', () => {
    expect(nextRunTime('2026-08-27T20:00:00Z')).toBe(NOW);
    expect(nextRunTime(null)).toBeNaN();
    expect(nextRunTime(undefined)).toBeNaN();
    expect(nextRunTime('')).toBeNaN();
  });
});

describe('msUntilNextMinute', () => {
  it('aligns to the next full minute', () => {
    expect(msUntilNextMinute(NOW)).toBe(60_000);
    expect(msUntilNextMinute(NOW + 42_000)).toBe(18_000);
    expect(msUntilNextMinute(NOW + 59_999)).toBe(1);
  });
});
