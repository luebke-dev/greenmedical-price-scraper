import { describe, expect, it } from 'vitest';
import { buildChartOption, type ChartTheme } from '@/lib/chart';
import {
  ALL_PRESET_DAYS,
  DEFAULT_PRESET,
  HISTORY_PRESETS,
  MAX_HISTORY_DAYS,
  UNIT_GRAM,
  UNIT_THC,
  buildSeries,
  formatHistoryAt,
  historyTableRows,
  isHistoryPreset,
  presetBucket,
  presetDays,
  presetRange,
  seriesAriaLabel,
} from '@/lib/history';
import { makeHistory, makePoint } from '../fixtures';
import { buildOfferHistoryParams } from '@/lib/history';

const NOW = new Date('2026-08-27T20:00:42Z');

describe('presets', () => {
  it('lists 7d/30d/90d/alles with 30d as default', () => {
    expect(HISTORY_PRESETS).toEqual(['7d', '30d', '90d', 'all']);
    expect(DEFAULT_PRESET).toBe('30d');
    expect(isHistoryPreset('all')).toBe(true);
    expect(isHistoryPreset('1y')).toBe(false);
  });

  it('maps presets to from/to/bucket (short spans per run, long spans per day)', () => {
    expect(presetDays('7d')).toBe(7);
    expect(presetDays('all')).toBe(ALL_PRESET_DAYS);
    expect(presetBucket('7d')).toBe('run');
    expect(presetBucket('30d')).toBe('run');
    expect(presetBucket('90d')).toBe('day');
    expect(presetBucket('all')).toBe('day');

    expect(presetRange('7d', NOW)).toEqual({
      from: '2026-08-20T20:00:00.000Z',
      to: '2026-08-27T20:00:00.000Z',
      bucket: 'run',
    });
    expect(presetRange('all', NOW)).toEqual({
      from: '2024-08-28T20:00:00.000Z',
      to: '2026-08-27T20:00:00.000Z',
      bucket: 'day',
    });
  });

  it('keeps "Alles" strictly inside the backend limit of 730 days', () => {
    expect(ALL_PRESET_DAYS).toBeLessThan(MAX_HISTORY_DAYS);
    const { from, to } = presetRange('all', NOW);
    const spanDays = (new Date(to).valueOf() - new Date(from).valueOf()) / 86_400_000;
    expect(spanDays).toBeLessThan(MAX_HISTORY_DAYS);
    expect(spanDays).toBe(ALL_PRESET_DAYS);
  });

  it('truncates "to" to the minute so cache keys are stable', () => {
    const a = presetRange('30d', new Date('2026-08-27T20:00:01Z'));
    const b = presetRange('30d', new Date('2026-08-27T20:00:59Z'));
    expect(a).toEqual(b);
  });
});

describe('formatHistoryAt', () => {
  it('formats run timestamps and calendar days', () => {
    expect(formatHistoryAt('2026-08-27T20:00:00Z', 'run')).toBe('27.08.2026, 22:00');
    expect(formatHistoryAt('2026-08-27', 'day')).toBe('27.08.2026');
  });
});

describe('buildSeries', () => {
  const points = [
    makePoint({
      run_id: 1,
      at: '2026-08-25T20:00:00Z',
      min: 5,
      avg: 6,
      max: 7,
      min_per_thc_gram: 20,
      avg_per_thc_gram: 22,
      max_per_thc_gram: 25,
    }),
    makePoint({
      run_id: 2,
      at: '2026-08-26T20:00:00Z',
      min: null,
      avg: null,
      max: null,
      min_per_thc_gram: null,
      avg_per_thc_gram: null,
      max_per_thc_gram: null,
      offer_count: 0,
      pharmacy_count: 0,
    }),
    makePoint({
      run_id: 3,
      at: '2026-08-27T20:00:00Z',
      min: 5.5,
      avg: 5.75,
      max: 6,
      min_per_thc_gram: 21,
      avg_per_thc_gram: 21.5,
      max_per_thc_gram: 22,
    }),
  ];
  const pharmacies = [
    {
      pharmacy_id: 1,
      name: 'Grüne Blüte',
      city: 'Markkleeberg',
      points: [
        {
          run_id: 1,
          at: '2026-08-25T20:00:00Z',
          price: 5,
          price_per_thc_gram: 20,
          availability: 'Auf Lager',
        },
        // run 2 missing → gap
        {
          run_id: 3,
          at: '2026-08-27T20:00:00Z',
          price: 5.5,
          price_per_thc_gram: 21,
          availability: 'NEU',
        },
      ],
    },
    {
      pharmacy_id: 2,
      name: 'Apo Zwei',
      city: '',
      points: [
        {
          run_id: 3,
          at: '2026-08-27T20:00:00Z',
          price: 6,
          price_per_thc_gram: 22,
          availability: 'Auf Lager',
        },
        {
          run_id: 99,
          at: '2027-01-01T00:00:00Z',
          price: 1,
          price_per_thc_gram: 1,
          availability: '',
        },
      ],
    },
  ];

  it('builds min/avg/max lines and the min–max band', () => {
    const series = buildSeries(makeHistory(points), { thcMode: false, pharmacies: false });
    expect(series.unit).toBe(UNIT_GRAM);
    expect(series.bucket).toBe('run');
    expect(series.keys).toEqual(points.map((p) => p.at));
    expect(series.categories).toEqual([
      '25.08.2026, 22:00',
      '26.08.2026, 22:00',
      '27.08.2026, 22:00',
    ]);
    expect(series.min).toEqual([5, null, 5.5]);
    expect(series.avg).toEqual([6, null, 5.75]);
    expect(series.max).toEqual([7, null, 6]);
    expect(series.bandLower).toEqual([5, null, 5.5]);
    expect(series.bandWidth).toEqual([2, null, 0.5]);
    expect(series.offerCount).toEqual([3, 0, 3]);
    expect(series.pharmacies).toEqual([]);
  });

  it('switches to €/g THC values in THC mode', () => {
    const series = buildSeries(makeHistory(points), { thcMode: true, pharmacies: false });
    expect(series.unit).toBe(UNIT_THC);
    expect(series.min).toEqual([20, null, 21]);
    expect(series.avg).toEqual([22, null, 21.5]);
    expect(series.max).toEqual([25, null, 22]);
    expect(series.bandWidth).toEqual([5, null, 1]);
  });

  it('aligns pharmacy series to the x axis and keeps gaps as null', () => {
    const series = buildSeries(makeHistory(points, pharmacies), {
      thcMode: false,
      pharmacies: true,
    });
    expect(series.pharmacies).toEqual([
      { id: 1, name: 'Grüne Blüte', city: 'Markkleeberg', data: [5, null, 5.5] },
      { id: 2, name: 'Apo Zwei', city: '', data: [null, null, 6] },
    ]);
    const thc = buildSeries(makeHistory(points, pharmacies), { thcMode: true, pharmacies: true });
    expect(thc.pharmacies[0]!.data).toEqual([20, null, 21]);
  });

  it('ignores pharmacy data when not requested or not delivered', () => {
    expect(
      buildSeries(makeHistory(points, pharmacies), { thcMode: false, pharmacies: false })
        .pharmacies,
    ).toEqual([]);
    expect(
      buildSeries(makeHistory(points), { thcMode: false, pharmacies: true }).pharmacies,
    ).toEqual([]);
  });

  it('uses calendar-day labels for day buckets', () => {
    const history = makeHistory([makePoint({ at: '2026-08-05', run_count: 4 })], undefined, {
      bucket: 'day',
    });
    const series = buildSeries(history, { thcMode: false, pharmacies: false });
    expect(series.categories).toEqual(['05.08.2026']);
  });

  it('produces the data-table fallback rows and an aria label', () => {
    const history = makeHistory(points);
    expect(historyTableRows(history, false)).toEqual([
      { at: '25.08.2026, 22:00', min: 5, avg: 6, max: 7, offerCount: 3, pharmacyCount: 3 },
      { at: '26.08.2026, 22:00', min: null, avg: null, max: null, offerCount: 0, pharmacyCount: 0 },
      { at: '27.08.2026, 22:00', min: 5.5, avg: 5.75, max: 6, offerCount: 3, pharmacyCount: 3 },
    ]);
    expect(historyTableRows(history, true)[0]).toMatchObject({ min: 20, avg: 22, max: 25 });
    const series = buildSeries(history, { thcMode: false, pharmacies: false });
    expect(seriesAriaLabel('OG Kush', series)).toBe(
      'Preisentwicklung von OG Kush in €/g: 3 Datenpunkte von 25.08.2026, 22:00 bis 27.08.2026, 22:00.',
    );
  });

  it('handles an empty history', () => {
    const series = buildSeries(makeHistory([]), { thcMode: false, pharmacies: true });
    expect(series.categories).toEqual([]);
    expect(series.min).toEqual([]);
    expect(seriesAriaLabel('X', series)).toContain('0 Datenpunkte');
  });
});

describe('buildChartOption', () => {
  const theme: ChartTheme = {
    text: '#111',
    muted: '#666',
    line: '#eee',
    lineStrong: '#ddd',
    surface: '#fff',
    accent: '#2f8f6b',
    accentStrong: '#1f6e51',
    amber: '#a06a16',
    band: 'rgba(0,0,0,.1)',
  };

  it('emits band + min/avg/max + pharmacy series with the theme colours', () => {
    const history = makeHistory(
      [makePoint({ at: '2026-08-27T20:00:00Z' })],
      [
        {
          pharmacy_id: 3,
          name: 'Apo',
          city: 'Leipzig',
          points: [
            { at: '2026-08-27T20:00:00Z', price: 6, price_per_thc_gram: 22, availability: '' },
          ],
        },
      ],
    );
    const series = buildSeries(history, { thcMode: false, pharmacies: true });
    const option = buildChartOption(series, theme, { animation: false }) as {
      animation: boolean;
      series: { id: string; name: string; data: unknown[] }[];
      legend: { data: string[] };
      yAxis: { name: string };
      xAxis: { data: string[] };
    };
    expect(option.animation).toBe(false);
    expect(option.series.map((s) => s.id)).toEqual([
      'band-lower',
      'band-width',
      'min',
      'avg',
      'max',
      'pharmacy-3',
    ]);
    expect(option.series[5]!.name).toBe('Apo (Leipzig)');
    expect(option.legend.data).toEqual(['Minimum', 'Durchschnitt', 'Maximum', 'Apo (Leipzig)']);
    expect(option.yAxis.name).toBe('€/g');
    expect(option.xAxis.data).toEqual(['27.08.2026, 22:00']);
  });
});

describe('buildOfferHistoryParams', () => {
  const range = {
    from: '2026-07-28T20:00:00.000Z',
    to: '2026-08-27T20:00:00.000Z',
    bucket: 'run',
  } as const;

  it('maps range, mode and page/size to limit/offset', () => {
    expect(buildOfferHistoryParams(range, { mode: 'changes', page: 1, size: 50 })).toEqual({
      ...range,
      mode: 'changes',
      limit: 50,
      offset: 0,
    });
    expect(buildOfferHistoryParams(range, { mode: 'all', page: 3, size: 25 })).toEqual({
      ...range,
      mode: 'all',
      limit: 25,
      offset: 50,
    });
    expect(buildOfferHistoryParams(range, { mode: 'all', page: 0, size: 100 }).offset).toBe(0);
  });
});
