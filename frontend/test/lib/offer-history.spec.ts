import { describe, expect, it } from 'vitest';
import type { History } from '@/api/types';
import { offerHistoryPhases, offerHistoryRows } from '@/lib/history';

const history: History = {
  strain_id: 7,
  bucket: 'run',
  from: '2026-08-20T00:00:00Z',
  to: '2026-08-27T00:00:00Z',
  timezone: 'Europe/Berlin',
  points: [
    {
      run_id: 1,
      at: '2026-08-26T08:00:00Z',
      min: 6.49,
      avg: 6.49,
      max: 6.49,
      min_per_thc_gram: null,
      avg_per_thc_gram: null,
      max_per_thc_gram: null,
      offer_count: 1,
      pharmacy_count: 1,
    },
    {
      run_id: 2,
      at: '2026-08-27T08:00:00Z',
      min: 5.99,
      avg: 6.74,
      max: 7.49,
      min_per_thc_gram: null,
      avg_per_thc_gram: null,
      max_per_thc_gram: null,
      offer_count: 2,
      pharmacy_count: 2,
    },
    {
      run_id: 3,
      at: '2026-08-27T14:00:00Z',
      min: 5.99,
      avg: 5.99,
      max: 5.99,
      min_per_thc_gram: null,
      avg_per_thc_gram: null,
      max_per_thc_gram: null,
      offer_count: 1,
      pharmacy_count: 1,
    },
  ],
  pharmacies: [
    {
      pharmacy_id: 1,
      name: 'Zeta Apotheke',
      city: 'Berlin',
      points: [
        {
          run_id: 1,
          at: '2026-08-26T08:00:00Z',
          price: 6.49,
          price_per_thc_gram: 27.04,
          availability: 'Auf Lager',
        },
        {
          run_id: 2,
          at: '2026-08-27T08:00:00Z',
          price: 5.99,
          price_per_thc_gram: 24.96,
          availability: 'NEU',
        },
        {
          run_id: 3,
          at: '2026-08-27T14:00:00Z',
          price: 5.99,
          price_per_thc_gram: 24.96,
          availability: 'NEU',
        },
      ],
    },
    {
      pharmacy_id: 2,
      name: 'Alpha Apotheke',
      city: 'Leipzig',
      points: [
        {
          run_id: 2,
          at: '2026-08-27T08:00:00Z',
          price: 7.49,
          price_per_thc_gram: 31.21,
          availability: 'Auf Lager',
        },
        {
          run_id: 3,
          at: '2026-08-27T14:00:00Z',
          price: null,
          price_per_thc_gram: null,
          availability: '',
        },
      ],
    },
  ],
};

describe('offerHistoryRows', () => {
  it('flattens pharmacy series newest run first, pharmacies alphabetically, skipping empty points', () => {
    const rows = offerHistoryRows(history);
    expect(rows.map((row) => [row.pharmacy, row.price, row.first])).toEqual([
      ['Zeta Apotheke', '5,99 €/g', true],
      ['Alpha Apotheke', '7,49 €/g', true],
      ['Zeta Apotheke', '5,99 €/g', false],
      ['Zeta Apotheke', '6,49 €/g', true],
    ]);
    expect(rows[1]!.thcPrice).toBe('31,21 €/g THC');
    expect(rows[0]!.availability).toBe('NEU');
    expect(rows[1]!.city).toBe('Leipzig');
    expect(new Set(rows.map((row) => row.key)).size).toBe(4);
  });

  it('returns no rows without pharmacy series', () => {
    const { pharmacies: _omit, ...withoutSeries } = history;
    void _omit;
    expect(offerHistoryRows(withoutSeries)).toEqual([]);
  });
});

describe('offerHistoryPhases', () => {
  it('collapses consecutive runs with the same price/status and marks delisting', () => {
    const phases = offerHistoryPhases(history);
    expect(phases.map((p) => [p.pharmacy, p.price, p.runs, p.to === null, p.delisted])).toEqual([
      // Alpha listed in run 2 only; run 3 (strain still seen) → delisted phase, current.
      ['Alpha Apotheke', '', 1, true, true],
      ['Alpha Apotheke', '7,49 €/g', 1, false, false],
      // Zeta: price change at run 2, unchanged in run 3 → one phase covering 2 runs, current.
      ['Zeta Apotheke', '5,99 €/g', 2, true, false],
      ['Zeta Apotheke', '6,49 €/g', 1, false, false],
    ]);
    expect(phases[2]!.availability).toBe('NEU');
    expect(phases[3]!.to).not.toBeNull();
  });

  it('returns nothing without runs', () => {
    expect(offerHistoryPhases({ ...history, points: [] })).toEqual([]);
  });
});
