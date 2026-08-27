import { describe, expect, it } from 'vitest';
import { COLUMNS, DEFAULT_SORT, ariaSort, isDefaultSort, isSortKey, toggleSort } from '@/lib/sort';

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
