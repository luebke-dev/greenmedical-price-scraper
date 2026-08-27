import { describe, expect, it } from 'vitest';
import {
  CHART_UPDATE_OPTIONS,
  buildChartOption,
  escapeHtml,
  readChartTheme,
  type ChartTheme,
} from '@/lib/chart';
import { buildSeries } from '@/lib/history';
import { makeHistory, makePoint } from '../fixtures';

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

type Formatter = (params: unknown) => string;

function tooltipFormatter(pharmacyName: string, city = ''): Formatter {
  const history = makeHistory(
    [makePoint({ at: '2026-08-27T20:00:00Z', min: 5, avg: 6, max: 7, offer_count: 4 })],
    [
      {
        pharmacy_id: 9,
        name: pharmacyName,
        city,
        points: [
          { at: '2026-08-27T20:00:00Z', price: 6, price_per_thc_gram: 22, availability: '' },
        ],
      },
    ],
  );
  const series = buildSeries(history, { thcMode: false, pharmacies: true });
  const option = buildChartOption(series, theme, { animation: false }) as {
    tooltip: { formatter: Formatter };
  };
  return option.tooltip.formatter;
}

describe('escapeHtml', () => {
  it('escapes the five HTML metacharacters and leaves the rest alone', () => {
    expect(escapeHtml(`<b>Apo & "Co" 'x'</b>`)).toBe(
      '&lt;b&gt;Apo &amp; &quot;Co&quot; &#39;x&#39;&lt;/b&gt;',
    );
    expect(escapeHtml('Grüne Blüte (Markkleeberg)')).toBe('Grüne Blüte (Markkleeberg)');
    expect(escapeHtml('')).toBe('');
  });
});

describe('tooltip formatter', () => {
  it('renders header, series lines and counts', () => {
    const formatter = tooltipFormatter('Apo', 'Leipzig');
    const html = formatter([
      {
        seriesId: 'min',
        seriesName: 'Minimum',
        axisValue: '27.08.2026, 22:00',
        dataIndex: 0,
        value: 5,
        marker: '<span class="m"></span>',
      },
      { seriesId: 'band-lower', seriesName: 'Spanne', value: 5 },
      { seriesId: 'band-width', seriesName: 'Spanne', value: 2 },
      { seriesId: 'pharmacy-9', seriesName: 'Apo (Leipzig)', value: null },
    ]);
    expect(html).toBe(
      [
        '<b>27.08.2026, 22:00</b>',
        '<span class="m"></span>Minimum: <b>5,00 €/g</b>',
        'Apo (Leipzig): <b>–</b>',
        '<span style="opacity:.75">Angebote: 4 · Apotheken: 3</span>',
      ].join('<br/>'),
    );
  });

  it('accepts a single (non-array) param', () => {
    const formatter = tooltipFormatter('Apo');
    expect(formatter({ seriesId: 'avg', seriesName: 'Durchschnitt', value: 6.5 })).toContain(
      'Durchschnitt: <b>6,50 €/g</b>',
    );
  });

  it('escapes scraped pharmacy names and the axis label before they reach innerHTML', () => {
    const name = '<img src=x onerror=alert(1)>';
    const formatter = tooltipFormatter(name, '<b>Leipzig</b>');
    const html = formatter([
      {
        seriesId: 'pharmacy-9',
        seriesName: `${name} (<b>Leipzig</b>)`,
        axisValue: '<script>x</script>',
        value: 6,
      },
    ]);
    expect(html).not.toContain('<img');
    expect(html).not.toContain('<script');
    expect(html).toContain('&lt;img src=x onerror=alert(1)&gt; (&lt;b&gt;Leipzig&lt;/b&gt;)');
    expect(html).toContain('<b>&lt;script&gt;x&lt;/script&gt;</b>');
  });
});

describe('CHART_UPDATE_OPTIONS', () => {
  it('merges by series id and drops vanished series', () => {
    expect(CHART_UPDATE_OPTIONS).toEqual({ notMerge: false, replaceMerge: ['series'] });
  });
});

describe('readChartTheme', () => {
  it('reads CSS custom properties from the element and falls back to the light palette', () => {
    const element = document.createElement('div');
    element.style.setProperty('--text', ' #abcdef ');
    element.style.setProperty('--chart-band', 'rgba(1, 2, 3, 0.5)');
    document.body.append(element);
    const result = readChartTheme(element);
    element.remove();
    expect(result.text).toBe('#abcdef');
    expect(result.band).toBe('rgba(1, 2, 3, 0.5)');
    expect(result.accent).toBe('#2f8f6b');
    expect(result.surface).toBe('#ffffff');
  });
});
