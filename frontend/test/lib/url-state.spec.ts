import { describe, expect, it } from 'vitest';
import type { BoundsState } from '@/lib/filter';
import {
  defaultFilterState,
  formatRange,
  fromQuery,
  parseRange,
  serializeQuery,
  toQuery,
  type FilterState,
} from '@/lib/url-state';

const bounds: BoundsState = {
  price: { min: 5.4, max: 12.4 },
  thc: { min: 18, max: 31 },
  cbd: { min: 0.9, max: 12 },
};

describe('url-state', () => {
  it('omits every default', () => {
    expect(toQuery(defaultFilterState(bounds), bounds)).toEqual({});
  });

  it('round-trips a full state', () => {
    const state: FilterState = {
      query: 'kush',
      genetik: ['indica', 'sativa'],
      ranges: { price: { lo: 6, hi: 9.9 }, thc: { lo: 18, hi: 31 }, cbd: { lo: 1, hi: 12 } },
      sort: { key: 'thc', direction: 'desc' },
    };
    const query = toQuery(state, bounds);
    expect(query).toEqual({
      q: 'kush',
      genetik: ['indica', 'sativa'],
      preis: '6-9.9',
      cbd: '1-12',
      sort: 'thc',
      dir: 'desc',
    });
    expect(fromQuery(query, { bounds })).toEqual(state);
  });

  it('omits dir for ascending non-default sorts', () => {
    const state = {
      ...defaultFilterState(bounds),
      sort: { key: 'name', direction: 'asc' } as const,
    };
    expect(toQuery(state, bounds)).toEqual({ sort: 'name' });
    expect(fromQuery({ sort: 'name' }, { bounds }).sort).toEqual({ key: 'name', direction: 'asc' });
  });

  it('clamps ranges into the bounds and swaps reversed values', () => {
    expect(parseRange('1-99', bounds.price!)).toEqual({ lo: 5.4, hi: 12.4 });
    expect(parseRange('9-6', bounds.price!)).toEqual({ lo: 6, hi: 9 });
    expect(fromQuery({ preis: '0-8' }, { bounds }).ranges.price).toEqual({ lo: 5.4, hi: 8 });
  });

  it('round-trips sort=rating', () => {
    const state = {
      ...defaultFilterState(bounds),
      sort: { key: 'rating', direction: 'desc' } as const,
    };
    expect(toQuery(state, bounds)).toEqual({ sort: 'rating', dir: 'desc' });
    expect(fromQuery({ sort: 'rating', dir: 'desc' }, { bounds }).sort).toEqual({
      key: 'rating',
      direction: 'desc',
    });
    expect(fromQuery({ sort: 'rating' }, { bounds }).sort).toEqual({
      key: 'rating',
      direction: 'asc',
    });
  });

  it('ignores malformed ranges, unknown sort keys and unknown directions', () => {
    const state = fromQuery(
      { preis: 'abc', thc: '1', sort: 'search', dir: 'sideways' },
      { bounds },
    );
    expect(state.ranges.price).toEqual({ lo: 5.4, hi: 12.4 });
    expect(state.ranges.thc).toEqual({ lo: 18, hi: 31 });
    expect(state.sort).toEqual({ key: 'price', direction: 'asc' });
    expect(fromQuery({ sort: 'thc', dir: 'sideways' }, { bounds }).sort).toEqual({
      key: 'thc',
      direction: 'asc',
    });
  });

  it('drops ranges for keys without bounds', () => {
    const state = fromQuery({ preis: '6-7' }, { bounds: { thc: bounds.thc! } });
    expect(state.ranges).toEqual({ thc: { lo: 18, hi: 31 } });
    expect(
      toQuery({ ...state, ranges: { price: { lo: 6, hi: 7 } } }, { thc: bounds.thc! }),
    ).toEqual({});
  });

  it('accepts genetik as string or array, lowercases and dedupes', () => {
    expect(fromQuery({ genetik: 'Indica' }, { bounds }).genetik).toEqual(['indica']);
    expect(
      fromQuery({ genetik: ['Indica', 'sativa', 'INDICA', null, ''] }, { bounds }).genetik,
    ).toEqual(['indica', 'sativa']);
  });

  it('drops unknown genetik keys when the known set is given', () => {
    const state = fromQuery(
      { genetik: ['indica', 'ruderalis'] },
      { bounds, genetikKeys: new Set(['indica']) },
    );
    expect(state.genetik).toEqual(['indica']);
  });

  it('takes the first value of repeated scalar params and ignores nulls', () => {
    expect(fromQuery({ q: ['kush', 'haze'] }, { bounds }).query).toBe('kush');
    expect(fromQuery({ q: null }, { bounds }).query).toBe('');
    expect(fromQuery({ q: undefined }, { bounds }).query).toBe('');
  });

  it('formats ranges with the step precision', () => {
    expect(formatRange({ lo: 5.4000000000000004, hi: 12.3 }, 0.1)).toBe('5.4-12.3');
    expect(formatRange({ lo: 18, hi: 27.5 }, 0.5)).toBe('18-27.5');
  });

  it('serializes queries deterministically', () => {
    expect(serializeQuery({ sort: 'thc', genetik: ['b', 'a'], q: 'x' })).toBe(
      'genetik=b&genetik=a&q=x&sort=thc',
    );
    expect(serializeQuery({})).toBe('');
  });
});
