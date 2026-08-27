import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import RefreshBanner from '@/components/RefreshBanner.vue';
import { installTestPlugins } from '../helpers';

installTestPlugins();

describe('RefreshBanner', () => {
  it('is a polite live region showing the countdown', () => {
    const wrapper = mount(RefreshBanner, {
      props: { phase: 'countdown', text: 'Nächste Aktualisierung in 37 Minuten', updated: false },
    });
    const banner = wrapper.find('.refresh-banner');
    expect(banner.attributes('role')).toBe('status');
    expect(banner.attributes('aria-live')).toBe('polite');
    expect(banner.attributes('style') ?? '').not.toContain('display: none');
    expect(banner.text()).toBe('Nächste Aktualisierung in 37 Minuten');
    expect(wrapper.find('.q-spinner').exists()).toBe(false);
  });

  it('shows a spinner while running', () => {
    const wrapper = mount(RefreshBanner, {
      props: { phase: 'running', text: 'Aktualisierung läuft …', updated: false },
    });
    expect(wrapper.find('.refresh-banner').classes()).toContain('is-running');
    expect(wrapper.find('.q-spinner').exists()).toBe(true);
    expect(wrapper.text()).toBe('Aktualisierung läuft …');
  });

  it('marks the overdue state', () => {
    const wrapper = mount(RefreshBanner, {
      props: { phase: 'overdue', text: 'Aktualisierung steht an …', updated: false },
    });
    expect(wrapper.find('.refresh-banner').classes()).toContain('is-overdue');
    expect(wrapper.text()).toBe('Aktualisierung steht an …');
  });

  it('is hidden without a schedule and shows the updated note on its own', async () => {
    const wrapper = mount(RefreshBanner, { props: { phase: 'hidden', text: '', updated: false } });
    expect(wrapper.find('.refresh-banner').attributes('style')).toContain('display: none');
    await wrapper.setProps({ updated: true });
    expect(wrapper.find('.refresh-banner').attributes('style') ?? '').not.toContain(
      'display: none',
    );
    expect(wrapper.text()).toBe('Daten aktualisiert');
  });

  it('appends the updated note to the countdown', () => {
    const wrapper = mount(RefreshBanner, {
      props: { phase: 'countdown', text: 'Nächste Aktualisierung in 59 Minuten', updated: true },
    });
    expect(wrapper.find('.refresh-updated').text()).toBe('Daten aktualisiert');
    expect(wrapper.text()).toContain('Nächste Aktualisierung in 59 Minuten');
  });
});
