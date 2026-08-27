import { describe, expect, it } from 'vitest';
import {
  MINUS,
  calendarDay,
  collator,
  dateOnly,
  dateTime,
  euro,
  fromEuro,
  integer,
  num,
  rating,
  ratingCompact,
  signedEuro,
  signedPercent,
} from '@/lib/format';

describe('format (de-DE)', () => {
  it('formats euro values with two decimals and a suffix', () => {
    expect(euro(5.49, '€/g')).toBe('5,49 €/g');
    expect(euro(1234.5, '€/g THC')).toBe('1.234,50 €/g THC');
    expect(euro(5, '€/g')).toBe('5,00 €/g');
  });

  it('returns an empty string for missing euro values', () => {
    expect(euro(null, '€/g')).toBe('');
    expect(euro(undefined, '€/g')).toBe('');
    expect(euro(Number.NaN, '€/g')).toBe('');
    expect(fromEuro(null, '€/g')).toBe('');
  });

  it('prefixes "ab" for table cells', () => {
    expect(fromEuro(6.29, '€/g')).toBe('ab 6,29 €/g');
  });

  it('formats numbers with a fixed decimal count', () => {
    expect(num(5.4, 2)).toBe('5,40');
    expect(num(27, 1)).toBe('27,0');
    expect(num(0.99, 1)).toBe('1,0');
  });

  it('formats integers with thousands separators', () => {
    expect(integer(2021)).toBe('2.021');
    expect(integer(18)).toBe('18');
  });

  it('formats signed deltas with a typographic minus', () => {
    expect(signedEuro(-0.5)).toBe(`${MINUS}0,50 €`);
    expect(signedEuro(0.5)).toBe('+0,50 €');
    expect(signedEuro(0)).toBe('±0,00 €');
    expect(signedPercent(-7.7)).toBe(`${MINUS}7,7 %`);
    expect(signedPercent(12.345)).toBe('+12,3 %');
    expect(signedPercent(0)).toBe('±0,0 %');
  });

  it('formats timestamps in de-DE (Europe/Berlin in tests)', () => {
    expect(dateTime('2026-08-27T20:00:00Z')).toBe('27.08.2026, 22:00');
    expect(dateOnly('2026-08-27T20:00:00Z')).toBe('27.08.2026');
    expect(dateTime('not a date')).toBe('');
    expect(dateTime(null)).toBe('');
    expect(dateTime(undefined)).toBe('');
  });

  it('formats API calendar days', () => {
    expect(calendarDay('2026-08-05')).toBe('05.08.2026');
    expect(calendarDay('garbage')).toBe('garbage');
  });

  it('uses a German, numeric, base-sensitive collator', () => {
    expect(collator.compare('Sorte 2', 'Sorte 10')).toBeLessThan(0);
    expect(collator.compare('äpfel', 'Apfel')).toBe(0);
    expect(collator.compare('a', 'A')).toBe(0);
    expect(collator.compare('Öl', 'Zucker')).toBeLessThan(0);
  });
});

describe('rating / ratingCompact', () => {
  it('formats one decimal in de-DE', () => {
    expect(rating(4.3)).toBe('4,3');
    expect(rating(5)).toBe('5,0');
    expect(rating(null)).toBe('');
    expect(rating(undefined)).toBe('');
  });

  it('renders "value (count)" and hides missing/empty ratings', () => {
    expect(ratingCompact(4.3, 124)).toBe('4,3 (124)');
    expect(ratingCompact(4.3, 1234)).toBe('4,3 (1.234)');
    expect(ratingCompact(null, 5)).toBe('');
    expect(ratingCompact(4.3, 0)).toBe('');
    expect(ratingCompact(4.3, undefined)).toBe('');
  });
});
