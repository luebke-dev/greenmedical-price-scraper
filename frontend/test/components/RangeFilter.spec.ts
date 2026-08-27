import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import RangeFilter from '@/components/RangeFilter.vue';
import { rangeConfig } from '@/lib/filter';
import { installTestPlugins } from '../helpers';

installTestPlugins();

function mountRange(overrides: Record<string, unknown> = {}) {
  return mount(RangeFilter, {
    props: {
      config: rangeConfig('price'),
      bounds: { min: 5.4, max: 12.4 },
      modelValue: { lo: 5.4, hi: 12.4 },
      ...overrides,
    },
  });
}

describe('RangeFilter', () => {
  it('renders label and the formatted "lo – hi" value', () => {
    const wrapper = mountRange();
    expect(wrapper.find('.filter-label').text()).toBe('Preis');
    expect(wrapper.find('.filter-value').text()).toBe('5,40 €/g – 12,40 €/g');
  });

  it('formats THC/CBD with one decimal and percent', () => {
    const wrapper = mountRange({
      config: rangeConfig('thc'),
      bounds: { min: 18, max: 31 },
      modelValue: { lo: 20, hi: 27.5 },
    });
    expect(wrapper.find('.filter-value').text()).toBe('20,0 % – 27,5 %');
  });

  it('renders two accessible slider thumbs with min/max/now', () => {
    const wrapper = mountRange({ modelValue: { lo: 6, hi: 9 } });
    const sliders = wrapper.findAll('[role="slider"]');
    expect(sliders).toHaveLength(2);
    expect(sliders[0]!.attributes('aria-label')).toBe('Preis Minimum');
    expect(sliders[1]!.attributes('aria-label')).toBe('Preis Maximum');
    expect(sliders[0]!.attributes('aria-valuenow')).toBe('6');
    expect(sliders[1]!.attributes('aria-valuenow')).toBe('9');
    expect(sliders[0]!.attributes('aria-valuemin')).toBe('5.4');
    expect(sliders[1]!.attributes('aria-valuemax')).toBe('12.4');
    expect(sliders[0]!.attributes('aria-valuetext')).toBe('6,00 €/g');
  });

  it('emits update:modelValue with rounded + clamped values', async () => {
    const wrapper = mountRange({ modelValue: { lo: 6, hi: 9 } });
    const range = wrapper.findComponent({ name: 'QRange' });
    range.vm.$emit('update:modelValue', { min: 6.000000001, max: 9.9 });
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('update:modelValue')).toEqual([[{ lo: 6, hi: 9.9 }]]);

    range.vm.$emit('update:modelValue', { min: 1, max: 99 });
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('update:modelValue')![1]).toEqual([{ lo: 5.4, hi: 12.4 }]);
  });

  it('does not emit when the value did not change', async () => {
    const wrapper = mountRange({ modelValue: { lo: 6, hi: 9 } });
    const range = wrapper.findComponent({ name: 'QRange' });
    range.vm.$emit('update:modelValue', { min: 6, max: 9 });
    await wrapper.vm.$nextTick();
    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
  });

  it('moves the thumbs with the keyboard', async () => {
    const wrapper = mountRange({ modelValue: { lo: 6, hi: 9 } });
    const [minThumb] = wrapper.findAll('[role="slider"]');
    await minThumb!.trigger('focus');
    await minThumb!.trigger('keydown', { keyCode: 39 }); // ArrowRight
    const emitted = wrapper.emitted('update:modelValue');
    expect(emitted).toBeDefined();
    expect(emitted![0]![0]).toEqual({ lo: 6.1, hi: 9 });
  });
});
