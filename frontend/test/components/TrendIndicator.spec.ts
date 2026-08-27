import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import TrendIndicator from '@/components/TrendIndicator.vue';
import { MINUS } from '@/lib/format';
import { LATEST_AT, makeTrend } from '../fixtures';
import { installTestPlugins } from '../helpers';

installTestPlugins();

describe('TrendIndicator', () => {
  it('renders nothing without a trend', () => {
    const wrapper = mount(TrendIndicator, { props: { trend: null, latestAt: LATEST_AT } });
    expect(wrapper.find('.trend').exists()).toBe(false);
    expect(wrapper.html()).not.toContain('▲');
  });

  it('renders ▼ (green) for a falling price with the tooltip text as aria-label', () => {
    const wrapper = mount(TrendIndicator, { props: { trend: makeTrend(), latestAt: LATEST_AT } });
    const el = wrapper.find('.trend');
    expect(el.text()).toBe('▼');
    expect(el.classes()).toContain('trend-down');
    expect(el.attributes('role')).toBe('img');
    expect(el.attributes('tabindex')).toBe('0');
    expect(el.attributes('data-direction')).toBe('down');
    expect(el.attributes('aria-label')).toBe(
      `Preis gefallen, vor 7 Tagen: 6,49 €/g (${MINUS}0,50 €, ${MINUS}7,7 %)`,
    );
    expect(wrapper.findComponent({ name: 'QTooltip' }).exists()).toBe(true);
  });

  it('renders ▲ (amber) for a rising price', () => {
    const wrapper = mount(TrendIndicator, {
      props: {
        trend: makeTrend({ direction: 'up', delta: 0.5, delta_pct: 7.7 }),
        latestAt: LATEST_AT,
      },
    });
    const el = wrapper.find('.trend');
    expect(el.text()).toBe('▲');
    expect(el.classes()).toContain('trend-up');
    expect(el.attributes('aria-label')).toBe(
      'Preis gestiegen, vor 7 Tagen: 6,49 €/g (+0,50 €, +7,7 %)',
    );
  });

  it('renders – (muted) for a flat price', () => {
    const wrapper = mount(TrendIndicator, {
      props: {
        trend: makeTrend({ direction: 'flat', delta: 0, delta_pct: 0 }),
        latestAt: LATEST_AT,
      },
    });
    const el = wrapper.find('.trend');
    expect(el.text()).toBe('–');
    expect(el.classes()).toContain('trend-flat');
    expect(el.attributes('aria-label')).toBe(
      'Preis unverändert, vor 7 Tagen: 6,49 €/g (±0,00 €, ±0,0 %)',
    );
  });
});
