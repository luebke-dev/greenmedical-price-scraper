import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import { nextTick } from 'vue';
import AppHeader from '@/components/AppHeader.vue';
import { useNavigationStore } from '@/stores/navigation';
import { makeMetadata } from '../fixtures';
import { installTestPlugins } from '../helpers';

installTestPlugins();

describe('AppHeader', () => {
  it('shows the run timestamp, the Preisalarm link and the API docs link', () => {
    const wrapper = mount(AppHeader, { props: { metadata: makeMetadata() } });
    expect(wrapper.find('h1').text()).toBe('GreenMedical Livebestand');
    expect(wrapper.find('time').text()).toBe('27.08.2026, 22:00');
    expect(wrapper.find('time').attributes('datetime')).toBe('2026-08-27T20:00:00Z');
    const links = wrapper.findAll('nav a');
    expect(links.map((link) => link.attributes('href'))).toEqual(['/abo', '/api/docs']);
    expect(links.map((link) => link.text())).toEqual(['Preisalarm', 'API']);
    expect(wrapper.text()).not.toContain('CSV');
    expect(wrapper.text()).not.toContain('JSON');
    const api = links[1]!;
    expect(api.attributes('target')).toBe('_blank');
    expect(api.attributes('rel')).toBe('noopener');
  });

  it('falls back to a dash without metadata', () => {
    const wrapper = mount(AppHeader, { props: { metadata: null } });
    expect(wrapper.find('time').exists()).toBe(false);
    expect(wrapper.find('.updated').text()).toBe('Stand: –');
  });

  it('links the brand to the remembered overview query', async () => {
    const wrapper = mount(AppHeader, { props: { metadata: null } });
    const brand = wrapper.find('a.brand-link');
    expect(brand.attributes('href')).toBe('/');

    useNavigationStore().rememberIndex({ genetik: ['indica'], sort: 'name', q: 'og' });
    await nextTick();
    expect(brand.attributes('href')).toBe('/?genetik=indica&sort=name&q=og');
  });
});
