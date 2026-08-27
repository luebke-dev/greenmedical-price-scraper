import { mount } from '@vue/test-utils';
import { describe, expect, it, vi } from 'vitest';
import { defineComponent, h } from 'vue';
import { buildSeries, historyTableRows } from '@/lib/history';
import { makeHistory, makePoint } from '../fixtures';
import { installTestPlugins } from '../helpers';

// ECharts needs a canvas; replace vue-echarts with a stub that records its props and instance.
const stub = vi.hoisted(() => ({ seq: 0 }));
vi.mock('vue-echarts', () => ({
  default: defineComponent({
    name: 'VChartStub',
    props: { option: { type: Object, required: true }, updateOptions: Object },
    setup(props) {
      const uid = ++stub.seq;
      return () =>
        h('div', {
          class: 'vchart-stub',
          'data-uid': String(uid),
          'data-update': JSON.stringify(props.updateOptions),
          'data-series': JSON.stringify(
            (props.option as { series: { id: string }[] }).series.map((s) => s.id),
          ),
        });
    },
  }),
}));

import PriceHistoryChart from '@/components/PriceHistoryChart.vue';

installTestPlugins();

const points = [
  makePoint({ run_id: 1, at: '2026-08-25T20:00:00Z', min: 5, avg: 6, max: 7 }),
  makePoint({ run_id: 2, at: '2026-08-27T20:00:00Z', min: 5.5, avg: 5.75, max: 6 }),
];
const pharmacies = [
  {
    pharmacy_id: 1,
    name: 'Apo',
    city: 'Leipzig',
    points: [
      { at: '2026-08-27T20:00:00Z', price: 6, price_per_thc_gram: 22, availability: 'Auf Lager' },
    ],
  },
];

function propsFor(
  options: { thcMode: boolean; pharmacies: boolean },
  history = makeHistory(points, pharmacies),
) {
  return {
    series: buildSeries(history, options),
    rows: historyTableRows(history, options.thcMode),
    label: 'Preisentwicklung von OG Kush',
  };
}

describe('PriceHistoryChart', () => {
  it('exposes the chart as an image with a label and the data as a table', () => {
    const wrapper = mount(PriceHistoryChart, {
      props: propsFor({ thcMode: false, pharmacies: false }),
    });
    const frame = wrapper.find('.chart-frame');
    expect(frame.attributes('role')).toBe('img');
    expect(frame.attributes('aria-label')).toBe('Preisentwicklung von OG Kush');
    expect(wrapper.find('.vchart-stub').attributes('aria-hidden')).toBe('true');

    const rows = wrapper.findAll('tbody tr');
    expect(rows).toHaveLength(2);
    expect(rows[0]!.findAll('td').map((td) => td.text())).toEqual([
      '25.08.2026, 22:00',
      '5,00 €/g',
      '6,00 €/g',
      '7,00 €/g',
      '3',
      '3',
    ]);
  });

  it('merges option updates by series id so zoom/legend state survives toggles', async () => {
    const wrapper = mount(PriceHistoryChart, {
      props: propsFor({ thcMode: false, pharmacies: true }),
    });
    const chart = () => wrapper.find('.vchart-stub');
    expect(JSON.parse(chart().attributes('data-update')!)).toEqual({
      notMerge: false,
      replaceMerge: ['series'],
    });
    expect(JSON.parse(chart().attributes('data-series')!)).toContain('pharmacy-1');
    const uid = chart().attributes('data-uid');

    // THC toggle / pharmacies off: same x axis → same chart instance, pharmacy series dropped.
    await wrapper.setProps(propsFor({ thcMode: true, pharmacies: false }));
    expect(chart().attributes('data-uid')).toBe(uid);
    expect(JSON.parse(chart().attributes('data-series')!)).not.toContain('pharmacy-1');
    expect(wrapper.find('tbody td.price').text()).toBe('20,33 €/g THC');
  });

  it('remounts the chart (fresh zoom) when the x axis changes', async () => {
    const wrapper = mount(PriceHistoryChart, {
      props: propsFor({ thcMode: false, pharmacies: false }),
    });
    const uid = wrapper.find('.vchart-stub').attributes('data-uid');
    const other = makeHistory([makePoint({ at: '2026-08-01', run_count: 4 })], undefined, {
      bucket: 'day',
    });
    await wrapper.setProps(propsFor({ thcMode: false, pharmacies: false }, other));
    expect(wrapper.find('.vchart-stub').attributes('data-uid')).not.toBe(uid);
    expect(wrapper.findAll('tbody tr')).toHaveLength(1);
    expect(wrapper.find('tbody td').text()).toBe('01.08.2026');
  });
});
