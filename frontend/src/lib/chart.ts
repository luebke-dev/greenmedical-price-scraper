// Pure ECharts option builder for the price history chart (no echarts runtime import).

import type { EChartsCoreOption, SetOptionOpts } from 'echarts/core';
import { de } from '@/i18n/de';
import { num } from './format';
import type { HistorySeries } from './history';

export interface ChartTheme {
  text: string;
  muted: string;
  line: string;
  lineStrong: string;
  surface: string;
  accent: string;
  accentStrong: string;
  amber: string;
  band: string;
}

/** Reads the CSS custom properties from css/tokens.scss for the current theme. */
export function readChartTheme(element: Element = document.body): ChartTheme {
  const styles = getComputedStyle(element);
  const token = (name: string, fallback: string) =>
    styles.getPropertyValue(name).trim() || fallback;
  return {
    text: token('--text', '#14201b'),
    muted: token('--muted', '#5a6a61'),
    line: token('--line', '#e3e8df'),
    lineStrong: token('--line-strong', '#d2dacb'),
    surface: token('--surface', '#ffffff'),
    accent: token('--accent', '#2f8f6b'),
    accentStrong: token('--accent-strong', '#1f6e51'),
    amber: token('--amber', '#a06a16'),
    band: token('--chart-band', 'rgba(47, 143, 107, 0.16)'),
  };
}

const PHARMACY_PALETTE = [
  '#5b8def',
  '#c96b9a',
  '#d98b2b',
  '#7a68d6',
  '#3fb0b0',
  '#b7761b',
  '#8f9a3c',
  '#d05c5c',
  '#4a9ad4',
  '#a06a9e',
];

export interface ChartOptions {
  animation: boolean;
}

/**
 * setOption flags for vue-echarts: merge updates (every series carries a stable id) so the
 * dataZoom window and legend selection survive the THC toggle and theme changes; `replaceMerge`
 * drops series that vanish (pharmacy lines switched off).
 */
export const CHART_UPDATE_OPTIONS: SetOptionOpts = { notMerge: false, replaceMerge: ['series'] };

interface TooltipParam {
  seriesName?: string;
  seriesId?: string;
  axisValue?: string;
  value?: unknown;
  marker?: string;
  dataIndex?: number;
}

function money(value: unknown, unit: string): string {
  return typeof value === 'number' && Number.isFinite(value) ? `${num(value, 2)} ${unit}` : '–';
}

const HTML_ESCAPES: Readonly<Record<string, string>> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
};

/**
 * The ECharts tooltip is rendered as HTML; series names (pharmacy name + city) come straight
 * from scraped data and must never reach innerHTML unescaped.
 */
export function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (char) => HTML_ESCAPES[char] ?? char);
}

export function buildChartOption(
  series: HistorySeries,
  theme: ChartTheme,
  options: ChartOptions,
): EChartsCoreOption {
  const unit = series.unit;
  const legendNames = [de.history.min, de.history.avg, de.history.max];

  const tooltipFormatter = (rawParams: TooltipParam | TooltipParam[]): string => {
    const params = Array.isArray(rawParams) ? rawParams : [rawParams];
    const header = escapeHtml(params[0]?.axisValue ?? '');
    const index = params[0]?.dataIndex ?? 0;
    const lines = params
      .filter((param) => param.seriesId !== 'band-lower' && param.seriesId !== 'band-width')
      .map(
        (param) =>
          // `marker` is ECharts' own markup; everything else is data and gets escaped.
          `${param.marker ?? ''}${escapeHtml(param.seriesName ?? '')}: <b>${money(param.value, unit)}</b>`,
      );
    const counts = `${de.history.offers}: ${series.offerCount[index] ?? 0} · ${de.history.pharmacyCount}: ${series.pharmacyCount[index] ?? 0}`;
    return [`<b>${header}</b>`, ...lines, `<span style="opacity:.75">${counts}</span>`].join(
      '<br/>',
    );
  };

  const pharmacySeries = series.pharmacies.map((pharmacy, index) => ({
    id: `pharmacy-${pharmacy.id}`,
    name: pharmacy.city ? `${pharmacy.name} (${pharmacy.city})` : pharmacy.name,
    type: 'line',
    data: pharmacy.data,
    connectNulls: false,
    showSymbol: false,
    symbol: 'circle',
    symbolSize: 5,
    lineStyle: { width: 1.5, color: PHARMACY_PALETTE[index % PHARMACY_PALETTE.length] },
    itemStyle: { color: PHARMACY_PALETTE[index % PHARMACY_PALETTE.length] },
    emphasis: { focus: 'series' },
    z: 3,
  }));

  return {
    animation: options.animation,
    backgroundColor: 'transparent',
    textStyle: { color: theme.text, fontFamily: 'inherit' },
    // ECharts 6: equivalent of the deprecated `containLabel: true`.
    grid: {
      left: 12,
      right: 16,
      top: 44,
      bottom: 56,
      outerBoundsMode: 'same',
      outerBoundsContain: 'axisLabel',
    },
    legend: {
      top: 0,
      data: [...legendNames, ...pharmacySeries.map((item) => item.name)],
      textStyle: { color: theme.muted },
      inactiveColor: theme.lineStrong,
      type: 'scroll',
      pageTextStyle: { color: theme.muted },
    },
    tooltip: {
      trigger: 'axis',
      backgroundColor: theme.surface,
      borderColor: theme.line,
      textStyle: { color: theme.text },
      formatter: tooltipFormatter,
      axisPointer: { type: 'line', lineStyle: { color: theme.lineStrong } },
    },
    xAxis: {
      type: 'category',
      boundaryGap: false,
      data: series.categories,
      axisLine: { lineStyle: { color: theme.line } },
      axisTick: { show: false },
      axisLabel: { color: theme.muted, hideOverlap: true },
    },
    yAxis: {
      type: 'value',
      scale: true,
      name: unit,
      nameTextStyle: { color: theme.muted, align: 'left' },
      axisLabel: { color: theme.muted, formatter: (value: number) => num(value, 2) },
      splitLine: { lineStyle: { color: theme.line } },
    },
    dataZoom: [
      { type: 'inside', filterMode: 'none' },
      {
        type: 'slider',
        height: 18,
        bottom: 8,
        borderColor: theme.line,
        backgroundColor: 'transparent',
        fillerColor: theme.band,
        handleStyle: { color: theme.accent },
        moveHandleStyle: { color: theme.accent },
        dataBackground: {
          lineStyle: { color: theme.lineStrong },
          areaStyle: { color: theme.line },
        },
        textStyle: { color: theme.muted },
        filterMode: 'none',
      },
    ],
    series: [
      {
        id: 'band-lower',
        name: de.history.band,
        type: 'line',
        data: series.bandLower,
        stack: 'band',
        stackStrategy: 'all',
        lineStyle: { opacity: 0 },
        symbol: 'none',
        silent: true,
        tooltip: { show: false },
        z: 1,
      },
      {
        id: 'band-width',
        name: de.history.band,
        type: 'line',
        data: series.bandWidth,
        stack: 'band',
        stackStrategy: 'all',
        lineStyle: { opacity: 0 },
        areaStyle: { color: theme.band },
        symbol: 'none',
        silent: true,
        tooltip: { show: false },
        z: 1,
      },
      {
        id: 'min',
        name: de.history.min,
        type: 'line',
        data: series.min,
        smooth: false,
        showSymbol: series.min.length <= 60,
        symbol: 'circle',
        symbolSize: 5,
        lineStyle: { width: 2.5, color: theme.accentStrong },
        itemStyle: { color: theme.accentStrong },
        z: 4,
      },
      {
        id: 'avg',
        name: de.history.avg,
        type: 'line',
        data: series.avg,
        showSymbol: false,
        lineStyle: { width: 1.5, color: theme.accent, type: 'dashed' },
        itemStyle: { color: theme.accent },
        z: 3,
      },
      {
        id: 'max',
        name: de.history.max,
        type: 'line',
        data: series.max,
        showSymbol: false,
        lineStyle: { width: 1.5, color: theme.amber },
        itemStyle: { color: theme.amber },
        z: 3,
      },
      ...pharmacySeries,
    ],
  };
}
