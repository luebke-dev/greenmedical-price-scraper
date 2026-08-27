import { describe, expect, it } from 'vitest';
import {
  RANGE_CONFIGS,
  boundsFromFacet,
  boundsFromFacets,
  buildStrainsParams,
  ceilToStep,
  clampRange,
  floorToStep,
  fullRanges,
  geneticsFromFacets,
  isFullRange,
  rangeConfig,
  roundToStep,
  stepDecimals,
  type StrainsQueryState,
} from '@/lib/filter';
import { makeFacets } from '../fixtures';

const price = rangeConfig('price');

describe('range configs', () => {
  it('uses the same keys, steps and decimals as site/app.js', () => {
    expect(RANGE_CONFIGS.map((c) => [c.key, c.step, c.decimals, c.unit])).toEqual([
      ['price', 0.1, 2, ' €/g'],
      ['thc', 0.5, 1, ' %'],
      ['cbd', 0.1, 1, ' %'],
    ]);
  });
});

describe('step rounding', () => {
  it('derives the decimal count from the step', () => {
    expect(stepDecimals(0.1)).toBe(1);
    expect(stepDecimals(0.5)).toBe(1);
    expect(stepDecimals(1)).toBe(0);
    expect(stepDecimals(0.25)).toBe(2);
  });

  it('removes floating point noise', () => {
    expect(roundToStep(5.4000000000000004, 0.1)).toBe(5.4);
    expect(roundToStep(0.30000000000000004, 0.1)).toBe(0.3);
  });

  it('floors and ceils to the step without drift on exact multiples', () => {
    expect(floorToStep(5.49, 0.1)).toBe(5.4);
    expect(ceilToStep(5.49, 0.1)).toBe(5.5);
    expect(floorToStep(5.5, 0.1)).toBe(5.5);
    expect(ceilToStep(5.5, 0.1)).toBe(5.5);
    expect(floorToStep(0.3, 0.1)).toBe(0.3);
    expect(ceilToStep(0.7, 0.1)).toBe(0.7);
    expect(floorToStep(27.3, 0.5)).toBe(27);
    expect(ceilToStep(27.3, 0.5)).toBe(27.5);
    expect(ceilToStep(0.99, 0.1)).toBe(1);
  });

  it('keeps exact multiples where app.js drifted (deliberate deviation)', () => {
    expect(Math.floor(0.3 / 0.1) * 0.1).toBe(0.2);
    expect(floorToStep(0.3, 0.1)).toBe(0.3);
    expect(ceilToStep(0.7, 0.1)).toBe(0.7);
    expect(floorToStep(1.1, 0.1)).toBe(1.1);
    expect(floorToStep(5.4, 0.1)).toBe(5.4);
  });
});

describe('bounds from facets', () => {
  it('floors/ceils the raw facet range to the step of each key', () => {
    expect(boundsFromFacet({ min: 5.49, max: 12.35 }, price)).toEqual({ min: 5.4, max: 12.4 });
    expect(boundsFromFacets(makeFacets())).toEqual({
      price: { min: 5.4, max: 12.4 },
      thc: { min: 18, max: 31 },
      cbd: { min: 0.3, max: 12 },
    });
    expect(fullRanges(boundsFromFacets(makeFacets()))).toEqual({
      price: { lo: 5.4, hi: 12.4 },
      thc: { lo: 18, hi: 31 },
      cbd: { lo: 0.3, hi: 12 },
    });
  });

  it('drops keys without a facet or without a span', () => {
    expect(boundsFromFacet(null, price)).toBeNull();
    expect(boundsFromFacet({ min: 7, max: 7 }, price)).toBeNull();
    expect(boundsFromFacet({ min: 7.01, max: 7.09 }, price)).toEqual({ min: 7, max: 7.1 });
    expect(boundsFromFacets(null)).toEqual({});
    expect(boundsFromFacets(makeFacets({ price: null, thc: { min: 20, max: 20 } }))).toEqual({
      cbd: { min: 0.3, max: 12 },
    });
  });

  it('isFullRange / clampRange', () => {
    const b = { min: 5.4, max: 12.4 };
    expect(isFullRange({ lo: 5.4, hi: 12.4 }, b)).toBe(true);
    expect(isFullRange({ lo: 5.3, hi: 13 }, b)).toBe(true);
    expect(isFullRange({ lo: 5.5, hi: 12.4 }, b)).toBe(false);
    expect(clampRange({ lo: 1, hi: 99 }, b)).toEqual({ lo: 5.4, hi: 12.4 });
    expect(clampRange({ lo: 9, hi: 6 }, b)).toEqual({ lo: 6, hi: 9 });
  });
});

describe('genetics from facets', () => {
  it('keeps the server order, lowercases the key and carries the count', () => {
    expect(geneticsFromFacets(makeFacets())).toEqual([
      { key: 'hybrid', label: 'Hybrid', count: 3 },
      { key: 'indica', label: 'Indica', count: 5 },
      { key: 'sativa', label: 'Sativa', count: 2 },
    ]);
  });

  it('dedupes case variants and returns nothing below two options', () => {
    expect(
      geneticsFromFacets(
        makeFacets({
          genetics: [
            { value: 'Indica', count: 1 },
            { value: 'INDICA', count: 1 },
            { value: '', count: 9 },
          ],
        }),
      ),
    ).toEqual([]);
    expect(geneticsFromFacets(null)).toEqual([]);
  });
});

describe('buildStrainsParams', () => {
  const bounds = boundsFromFacets(makeFacets());
  const base: StrainsQueryState = {
    query: '',
    genetics: [],
    ranges: fullRanges(bounds),
    sort: { key: 'price', direction: 'asc' },
    page: 1,
    size: 50,
  };

  it('sends only sort/dir/limit/offset for the default state', () => {
    expect(buildStrainsParams(base, bounds)).toEqual({
      sort: 'price',
      dir: 'asc',
      limit: 50,
      offset: 0,
    });
  });

  it('omits ranges at the full slider width and sends only the moved side', () => {
    const state: StrainsQueryState = {
      ...base,
      ranges: {
        price: { lo: 6, hi: 12.4 },
        thc: { lo: 18, hi: 25.5 },
        cbd: { lo: 1, hi: 4.4 },
      },
    };
    expect(buildStrainsParams(state, bounds)).toMatchObject({
      price_min: 6,
      thc_max: 25.5,
      cbd_min: 1,
      cbd_max: 4.4,
    });
    expect(buildStrainsParams(state, bounds)).not.toHaveProperty('price_max');
    expect(buildStrainsParams(state, bounds)).not.toHaveProperty('thc_min');
  });

  it('clamps values outside the bounds', () => {
    const state: StrainsQueryState = { ...base, ranges: { price: { lo: 1, hi: 8 } } };
    expect(buildStrainsParams(state, bounds)).toMatchObject({ price_max: 8 });
    expect(buildStrainsParams(state, bounds)).not.toHaveProperty('price_min');
  });

  it('passes ranges through as given while the bounds are unknown (cold-start deep link)', () => {
    const state: StrainsQueryState = { ...base, ranges: { price: { lo: 6, hi: 8 } } };
    expect(buildStrainsParams(state, {})).toMatchObject({ price_min: 6, price_max: 8 });
  });

  it('maps query, genetics, sort and page/size', () => {
    const state: StrainsQueryState = {
      ...base,
      query: '  kush ',
      genetics: ['indica', 'sativa'],
      sort: { key: 'rating', direction: 'desc' },
      page: 3,
      size: 25,
    };
    expect(buildStrainsParams(state, bounds)).toEqual({
      q: 'kush',
      genetics: ['indica', 'sativa'],
      sort: 'rating',
      dir: 'desc',
      limit: 25,
      offset: 50,
    });
  });
});
