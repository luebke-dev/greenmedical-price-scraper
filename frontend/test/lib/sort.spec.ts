import { describe, expect, it } from 'vitest';
import {
  COLUMNS,
  DEFAULT_SORT,
  ariaSort,
  compareRows,
  getSortValue,
  isDefaultSort,
  isSortKey,
  sortRows,
  toggleSort,
} from '@/lib/sort';
import { makeStrain } from '../fixtures';

describe('columns', () => {
  it('defines the 9 columns of the table in order', () => {
    expect(COLUMNS.map((c) => c.key)).toEqual([
      'name',
      'bezeichnung',
      'price',
      'price_per_thc_gram',
      'thc',
      'cbd',
      'genetik',
      'pharmacy_count',
      'rating',
    ]);
    expect(COLUMNS.map((c) => c.label)).toEqual([
      'Sorte',
      'Bezeichnung',
      'ab €/g',
      'ab €/g THC',
      'THC',
      'CBD',
      'Genetik',
      'Apotheken',
      'Bewertung',
    ]);
  });

  it('defaults to price ascending', () => {
    expect(DEFAULT_SORT).toEqual({ key: 'price', direction: 'asc' });
    expect(isDefaultSort({ key: 'price', direction: 'asc' })).toBe(true);
    expect(isDefaultSort({ key: 'price', direction: 'desc' })).toBe(false);
    expect(isSortKey('thc')).toBe(true);
    expect(isSortKey('search')).toBe(false);
  });
});

describe('getSortValue', () => {
  it('reads numeric keys from sort/pharmacy_count and text keys from the row', () => {
    const row = makeStrain({
      name: 'OG Kush',
      price: 6.49,
      thcPrice: 27.04,
      thcValue: 24,
      cbdValue: 1,
    });
    expect(getSortValue(row, 'price')).toBe(6.49);
    expect(getSortValue(row, 'price_per_thc_gram')).toBe(27.04);
    expect(getSortValue(row, 'thc')).toBe(24);
    expect(getSortValue(row, 'cbd')).toBe(1);
    expect(getSortValue(row, 'pharmacy_count')).toBe(1);
    expect(getSortValue(row, 'name')).toBe('OG Kush');
    expect(getSortValue(makeStrain({ genetik: '' }), 'genetik')).toBe('');
  });
});

describe('sortRows', () => {
  const rows = [
    makeStrain({ id: 1, name: 'Zeta', price: 8, thcValue: null }),
    makeStrain({ id: 2, name: 'alpha', price: null, thcValue: 20 }),
    makeStrain({ id: 3, name: 'Ärger', price: 5.49, thcValue: 30 }),
    makeStrain({ id: 4, name: 'Sorte 10', price: 6.5, thcValue: 25 }),
    makeStrain({ id: 5, name: 'Sorte 2', price: 6.5, thcValue: 25 }),
  ];

  it('sorts numbers ascending with nulls last', () => {
    expect(sortRows(rows, { key: 'price', direction: 'asc' }).map((r) => r.id)).toEqual([
      3, 4, 5, 1, 2,
    ]);
  });

  it('sorts numbers descending with nulls STILL last (deliberate deviation from app.js)', () => {
    expect(sortRows(rows, { key: 'price', direction: 'desc' }).map((r) => r.id)).toEqual([
      1, 4, 5, 3, 2,
    ]);
    expect(sortRows(rows, { key: 'thc', direction: 'desc' }).map((r) => r.id)).toEqual([
      3, 4, 5, 2, 1,
    ]);
    expect(sortRows(rows, { key: 'thc', direction: 'asc' }).map((r) => r.id)).toEqual([
      2, 4, 5, 3, 1,
    ]);
  });

  it('sorts text with the German collator (case-insensitive, umlauts, numeric)', () => {
    expect(sortRows(rows, { key: 'name', direction: 'asc' }).map((r) => r.name)).toEqual([
      'alpha',
      'Ärger',
      'Sorte 2',
      'Sorte 10',
      'Zeta',
    ]);
    expect(sortRows(rows, { key: 'name', direction: 'desc' }).map((r) => r.name)).toEqual([
      'Zeta',
      'Sorte 10',
      'Sorte 2',
      'Ärger',
      'alpha',
    ]);
  });

  it('is stable and does not mutate the input', () => {
    const copy = [...rows];
    const sorted = sortRows(rows, { key: 'price', direction: 'asc' });
    expect(rows).toEqual(copy);
    expect(sorted).not.toBe(rows);
    // ids 4 and 5 share the price → original order kept
    expect(sorted.map((r) => r.id).indexOf(4)).toBeLessThan(sorted.map((r) => r.id).indexOf(5));
  });

  it('compareRows treats two nulls as equal', () => {
    const a = makeStrain({ id: 1, price: null });
    const b = makeStrain({ id: 2, price: null });
    expect(compareRows(a, b, { key: 'price', direction: 'asc' })).toBe(0);
    expect(compareRows(a, b, { key: 'price', direction: 'desc' })).toBe(0);
  });
});

describe('toggleSort / ariaSort', () => {
  it('flips the direction on the same key, resets to asc on another key', () => {
    expect(toggleSort({ key: 'price', direction: 'asc' }, 'price')).toEqual({
      key: 'price',
      direction: 'desc',
    });
    expect(toggleSort({ key: 'price', direction: 'desc' }, 'price')).toEqual({
      key: 'price',
      direction: 'asc',
    });
    expect(toggleSort({ key: 'price', direction: 'desc' }, 'name')).toEqual({
      key: 'name',
      direction: 'asc',
    });
  });

  it('starts the rating column descending, then toggles like every other column', () => {
    expect(toggleSort({ key: 'price', direction: 'asc' }, 'rating')).toEqual({
      key: 'rating',
      direction: 'desc',
    });
    expect(toggleSort({ key: 'rating', direction: 'desc' }, 'rating')).toEqual({
      key: 'rating',
      direction: 'asc',
    });
    expect(toggleSort({ key: 'rating', direction: 'asc' }, 'price')).toEqual({
      key: 'price',
      direction: 'asc',
    });
  });

  it('maps to aria-sort values', () => {
    expect(ariaSort({ key: 'price', direction: 'asc' }, 'price')).toBe('ascending');
    expect(ariaSort({ key: 'price', direction: 'desc' }, 'price')).toBe('descending');
    expect(ariaSort({ key: 'price', direction: 'asc' }, 'name')).toBe('none');
  });
});

describe('sort by rating', () => {
  const rows = [
    makeStrain({ id: 1, name: 'A', ratingValue: 4.1, reviewCount: 10 }),
    makeStrain({ id: 2, name: 'B' }), // never scraped → rating null
    makeStrain({ id: 3, name: 'C', ratingValue: 4.8, reviewCount: 3 }),
    makeStrain({ id: 4, name: 'D', ratingValue: null, reviewCount: 0 }), // scraped, no reviews
    makeStrain({ id: 5, name: 'E', ratingValue: 3.5, reviewCount: 50 }),
  ];

  it('reads sort.rating', () => {
    expect(getSortValue(rows[0]!, 'rating')).toBe(4.1);
    expect(getSortValue(rows[1]!, 'rating')).toBeNull();
    expect(getSortValue(rows[3]!, 'rating')).toBeNull();
    expect(isSortKey('rating')).toBe(true);
  });

  it('puts strains without a rating last in both directions', () => {
    const desc = sortRows(rows, { key: 'rating', direction: 'desc' }).map((r) => r.name);
    expect(desc).toEqual(['C', 'A', 'E', 'B', 'D']);
    const asc = sortRows(rows, { key: 'rating', direction: 'asc' }).map((r) => r.name);
    expect(asc).toEqual(['E', 'A', 'C', 'B', 'D']);
  });
});
