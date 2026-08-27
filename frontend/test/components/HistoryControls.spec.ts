import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import HistoryControls from '@/components/HistoryControls.vue';
import { installTestPlugins } from '../helpers';

installTestPlugins();

function mountControls(overrides: Record<string, unknown> = {}) {
  return mount(HistoryControls, {
    props: { preset: '30d', thcMode: false, pharmacies: false, ...overrides },
  });
}

describe('HistoryControls', () => {
  it('renders the preset toggle as a labelled group with the active preset pressed', () => {
    const wrapper = mountControls();
    const group = wrapper.find('.preset-toggle');
    expect(group.attributes('role')).toBe('group');
    expect(group.attributes('aria-label')).toBe('Zeitraum');

    const buttons = group.findAll('button');
    expect(buttons.map((button) => button.text())).toEqual([
      '7 Tage',
      '30 Tage',
      '90 Tage',
      'Alles',
    ]);
    expect(buttons.map((button) => button.attributes('aria-pressed'))).toEqual([
      'false',
      'true',
      'false',
      'false',
    ]);
  });

  it('emits update:preset when another preset is chosen', async () => {
    const wrapper = mountControls();
    const buttons = wrapper.find('.preset-toggle').findAll('button');
    await buttons[0]!.trigger('click');
    expect(wrapper.emitted('update:preset')).toEqual([['7d']]);
    await buttons[3]!.trigger('click');
    expect(wrapper.emitted('update:preset')).toEqual([['7d'], ['all']]);
  });

  it('emits the THC and pharmacy toggles', async () => {
    const wrapper = mountControls({ pharmacies: true });
    const toggles = wrapper.findAll('.q-toggle');
    expect(toggles).toHaveLength(2);
    expect(toggles[0]!.text()).toBe('€/g THC');
    expect(toggles[1]!.text()).toBe('Apotheken einzeln');
    expect(toggles[0]!.attributes('aria-checked')).toBe('false');
    expect(toggles[1]!.attributes('aria-checked')).toBe('true');

    await toggles[0]!.trigger('click');
    expect(wrapper.emitted('update:thcMode')).toEqual([[true]]);
    await toggles[1]!.trigger('click');
    expect(wrapper.emitted('update:pharmacies')).toEqual([[false]]);
  });
});
