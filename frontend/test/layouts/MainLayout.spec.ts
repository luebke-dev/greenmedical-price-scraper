import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { getMetadata } from '@/api/endpoints';
import MainLayout from '@/layouts/MainLayout.vue';
import { makeMetadata } from '../fixtures';
import { installTestPlugins } from '../helpers';

vi.mock('@/api/endpoints', () => ({
  API_DOCS_URL: '/api/docs',
  getMetadata: vi.fn(),
  getStrains: vi.fn(),
  getStrain: vi.fn(),
}));

installTestPlugins();

const metadataMock = vi.mocked(getMetadata);

describe('MainLayout', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-08-27T20:23:00Z'));
    metadataMock.mockReset();
    metadataMock.mockResolvedValue(makeMetadata({ next_run_at: '2026-08-27T21:00:00Z' }));
  });
  afterEach(() => vi.useRealTimers());

  it('renders the refresh banner above the header', async () => {
    const wrapper = mount(MainLayout);
    await flushPromises();
    const banner = wrapper.find('.refresh-banner');
    expect(banner.exists()).toBe(true);
    expect(banner.text()).toBe('Nächste Aktualisierung in 37 Minuten');
    const main = wrapper.find('main.page').element;
    expect(main.children[0]!.classList.contains('refresh-banner')).toBe(true);
    expect(main.children[1]!.classList.contains('page-header')).toBe(true);
    expect(wrapper.find('.updated').text()).toBe('Stand: 27.08.2026, 22:00');
  });
});
