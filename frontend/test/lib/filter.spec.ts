import { describe, expect, it } from 'vitest';
import {
  RANGE_CONFIGS,
  applyFilters,
  ceilToStep,
  clampRange,
  computeAllBounds,
  computeBounds,
  floorToStep,
  fullRanges,
  genetikOptions,
  isFullRange,
  matchesGenetik,
  matchesRange,
  rangeConfig,
  roundToStep,
  stepDecimals,
} from '@/lib/filter';
import { matchesSearch, normalizeQuery } from '@/lib/search';
import { makeStrain } from '../fixtures';

const price = rangeConfig('price');
const thc = rangeConfig('thc');
const cbd = rangeConfig('cbd');

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
    // app.js: Math.floor(0.3 / 0.1) * 0.1 === 0.2 and Math.ceil(0.7 / 0.1) * 0.1 === 0.7000000000000001
    expect(Math.floor(0.3 / 0.1) * 0.1).toBe(0.2);
    expect(floorToStep(0.3, 0.1)).toBe(0.3);
    expect(ceilToStep(0.7, 0.1)).toBe(0.7);
    expect(floorToStep(1.1, 0.1)).toBe(1.1);
    expect(floorToStep(5.4, 0.1)).toBe(5.4);
  });
});

describe('computeBounds', () => {
  it('floors min and ceils max to the step', () => {
    const rows = [makeStrain({ id: 1, price: 5.49 }), makeStrain({ id: 2, price: 12.31 })];
    expect(computeBounds(rows, price)).toEqual({ min: 5.4, max: 12.4 });
  });

  it('ignores null values', () => {
    const rows = [makeStrain({ id: 1, price: null }), makeStrain({ id: 2, price: 7 })];
    expect(computeBounds(rows, price)).toBeNull();
    rows.push(makeStrain({ id: 3, price: 8.05 }));
    expect(computeBounds(rows, price)).toEqual({ min: 7, max: 8.1 });
  });

  it('returns null when there are no values or min === max', () => {
    expect(computeBounds([], price)).toBeNull();
    expect(computeBounds([makeStrain({ price: null })], price)).toBeNull();
    const same = [makeStrain({ id: 1, price: 6 }), makeStrain({ id: 2, price: 6 })];
    expect(computeBounds(same, price)).toBeNull();
  });

  it('computes bounds per key with the respective steps', () => {
    const rows = [
      makeStrain({ id: 1, price: 5.49, thcValue: 27, cbdValue: 0.99 }),
      makeStrain({ id: 2, price: 9.9, thcValue: 18.2, cbdValue: 12 }),
    ];
    expect(computeAllBounds(rows)).toEqual({
      price: { min: 5.4, max: 9.9 },
      thc: { min: 18, max: 27 },
      cbd: { min: 0.9, max: 12 },
    });
    expect(fullRanges(computeAllBounds(rows))).toEqual({
      price: { lo: 5.4, hi: 9.9 },
      thc: { lo: 18, hi: 27 },
      cbd: { lo: 0.9, hi: 12 },
    });
  });
});

describe('matchesRange', () => {
  const rows = [
    makeStrain({ id: 1, price: 5.49 }),
    makeStrain({ id: 2, price: 8 }),
    makeStrain({ id: 3, price: null }),
  ];
  const bounds = computeAllBounds(rows);

  it('keeps null rows only while the filter is at its full range', () => {
    const full = fullRanges(bounds);
    expect(rows.map((row) => matchesRange(row, full, bounds))).toEqual([true, true, true]);

    const narrowed = { ...full, price: { lo: 5.4, hi: 7.9 } };
    expect(rows.map((row) => matchesRange(row, narrowed, bounds))).toEqual([true, false, false]);

    // Slightly moved lower thumb → still not "full range" → null rows disappear.
    const nudged = { ...full, price: { lo: 5.5, hi: bounds.price!.max } };
    expect(rows.map((row) => matchesRange(row, nudged, bounds))).toEqual([false, true, false]);
  });

  it('ignores keys without bounds', () => {
    expect(matchesRange(rows[2]!, { price: { lo: 6, hi: 7 } }, {})).toBe(true);
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

describe('genetik', () => {
  it('collects distinct values case-insensitively and sorts them', () => {
    const rows = [
      makeStrain({ id: 1, genetik: 'Sativa' }),
      makeStrain({ id: 2, genetik: 'indica' }),
      makeStrain({ id: 3, genetik: 'Indica' }),
      makeStrain({ id: 4, genetik: 'Hybrid Sativa Dominant' }),
      makeStrain({ id: 5, genetik: '' }),
    ];
    // Like site/app.js: the label shows the casing of the LAST occurrence.
    expect(genetikOptions(rows)).toEqual([
      { key: 'hybrid sativa dominant', label: 'Hybrid Sativa Dominant' },
      { key: 'indica', label: 'Indica' },
      { key: 'sativa', label: 'Sativa' },
    ]);
  });

  it('returns no options for fewer than two distinct values', () => {
    expect(
      genetikOptions([makeStrain({ genetik: 'Indica' }), makeStrain({ genetik: 'INDICA' })]),
    ).toEqual([]);
    expect(genetikOptions([])).toEqual([]);
  });

  it('matches case-folded and passes everything when nothing is selected', () => {
    const row = makeStrain({ genetik: 'Indica' });
    expect(matchesGenetik(row, new Set())).toBe(true);
    expect(matchesGenetik(row, new Set(['indica']))).toBe(true);
    expect(matchesGenetik(row, new Set(['sativa']))).toBe(false);
    expect(matchesGenetik(makeStrain({ genetik: '' }), new Set(['indica']))).toBe(false);
  });
});

describe('search', () => {
  it('normalizes the query (trim + lowercase)', () => {
    expect(normalizeQuery('  Grüne BLÜTE ')).toBe('grüne blüte');
  });

  it('matches substrings of the precomputed search field', () => {
    const row = makeStrain({ name: 'OG Kush', bezeichnung: 'Cannamedical CM 24/1' });
    expect(matchesSearch(row, 'kush')).toBe(true);
    expect(matchesSearch(row, 'CM 24')).toBe(true);
    expect(matchesSearch(row, 'markkleeberg')).toBe(true);
    expect(matchesSearch(row, 'haze')).toBe(false);
    expect(matchesSearch(row, '')).toBe(true);
    expect(matchesSearch(row, '   ')).toBe(true);
  });
});

describe('applyFilters', () => {
  const rows = [
    makeStrain({ id: 1, name: 'Alpha', genetik: 'Indica', price: 5.49, thcValue: 27, cbdValue: 1 }),
    makeStrain({ id: 2, name: 'Beta', genetik: 'Sativa', price: 8, thcValue: 20, cbdValue: 0.99 }),
    makeStrain({
      id: 3,
      name: 'Gamma',
      genetik: 'Indica',
      price: null,
      thcValue: null,
      cbdValue: 5,
    }),
  ];
  const bounds = computeAllBounds(rows);

  it('combines search, genetik and ranges', () => {
    const all = applyFilters(
      rows,
      { query: '', genetik: new Set(), ranges: fullRanges(bounds) },
      bounds,
    );
    expect(all.map((r) => r.id)).toEqual([1, 2, 3]);

    const indica = applyFilters(
      rows,
      { query: '', genetik: new Set(['indica']), ranges: fullRanges(bounds) },
      bounds,
    );
    expect(indica.map((r) => r.id)).toEqual([1, 3]);

    const cheap = applyFilters(
      rows,
      {
        query: '',
        genetik: new Set(),
        ranges: { ...fullRanges(bounds), price: { lo: 5.4, hi: 6 } },
      },
      bounds,
    );
    expect(cheap.map((r) => r.id)).toEqual([1]);

    const searched = applyFilters(
      rows,
      { query: 'gam', genetik: new Set(), ranges: fullRanges(bounds) },
      bounds,
    );
    expect(searched.map((r) => r.id)).toEqual([3]);
  });

  it('keeps the original order (sorting is separate)', () => {
    const reversed = [...rows].reverse();
    const result = applyFilters(reversed, { query: '', genetik: new Set(), ranges: {} }, bounds);
    expect(result.map((r) => r.id)).toEqual([3, 2, 1]);
  });

  it('thc/cbd bounds use their own steps', () => {
    expect(bounds.thc).toEqual({ min: 20, max: 27 });
    expect(bounds.cbd).toEqual({ min: 0.9, max: 5 });
    expect(thc.get(rows[2]!)).toBeNull();
    expect(cbd.get(rows[1]!)).toBe(0.99);
  });
});
