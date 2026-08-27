import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import EmptyState from '@/components/EmptyState.vue';

describe('EmptyState', () => {
  it('is a polite status by default', () => {
    const wrapper = mount(EmptyState, { props: { message: 'Keine Sorten gefunden.' } });
    expect(wrapper.attributes('role')).toBe('status');
    expect(wrapper.classes()).not.toContain('empty--error');
    expect(wrapper.text()).toBe('Keine Sorten gefunden.');
  });

  it('announces errors as an alert', () => {
    const wrapper = mount(EmptyState, {
      props: { message: 'Daten konnten nicht geladen werden.', tone: 'error' },
    });
    expect(wrapper.attributes('role')).toBe('alert');
    expect(wrapper.classes()).toContain('empty--error');
    expect(wrapper.find('button').exists()).toBe(false);
  });

  it('offers a retry button when a label is given', async () => {
    const wrapper = mount(EmptyState, {
      props: {
        message: 'Daten konnten nicht geladen werden.',
        tone: 'error',
        retryLabel: 'Erneut laden',
      },
    });
    const button = wrapper.find('button.empty-retry');
    expect(button.text()).toBe('Erneut laden');
    expect(button.attributes('type')).toBe('button');
    expect(wrapper.find('.empty-message').text()).toBe('Daten konnten nicht geladen werden.');
    await button.trigger('click');
    expect(wrapper.emitted('retry')).toEqual([[]]);
  });
});
