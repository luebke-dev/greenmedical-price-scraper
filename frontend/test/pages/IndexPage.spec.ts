import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Router } from 'vue-router';
import { getMetadata, getStrains, type StrainsParams } from '@/api/endpoints';
import IndexPage from '@/pages/IndexPage.vue';
import { SEARCH_DEBOUNCE_MS } from '@/composables/useStrainFilters';
import { makeListItem, makeMetadata, makeStrainsPage } from '../fixtures';
import { createTestPinia, createTestRouter } from '../helpers';
import { installQuasarPlugin } from '@quasar/quasar-app-extension-testing-unit-vitest';

vi.mock('@/api/endpoints', () => ({
  getStrains: vi.fn(),
  getMetadata: vi.fn(),
  getStrain: vi.fn(),
}));

installQuasarPlugin();

const strainsMock = vi.mocked(getStrains);
const metadataMock = vi.mocked(getMetadata);

function lastParams(): StrainsParams | undefined {
  return strainsMock.mock.calls[strainsMock.mock.calls.length - 1]?.[0];
}

describe('IndexPage', () => {
  let router: Router;

  beforeEach(async () => {
    vi.useFakeTimers();
    strainsMock.mockReset();
    metadataMock.mockReset();
    metadataMock.mockResolvedValue(makeMetadata());
    strainsMock.mockImplementation((params) => {
      const offset = params?.offset ?? 0;
      const limit = params?.limit ?? 50;
      const rows = Array.from({ length: Math.min(limit, 130 - offset) }, (_, index) =>
        makeListItem({ id: offset + index + 1, name: `Sorte ${offset + index + 1}` }),
      );
      return Promise.resolve(makeStrainsPage(rows, { total: 130, limit, offset }));
    });
    router = createTestRouter();
    await router.push('/');
    await router.isReady();
  });

  afterEach(() => vi.useRealTimers());

  async function mountPage() {
    const pinia = createTestPinia();
    const wrapper = mount(IndexPage, { global: { plugins: [router, pinia] } });
    await flushPromises();
    return wrapper;
  }

  it('loads the first page, shows the total and pages through the server', async () => {
    const wrapper = await mountPage();
    expect(lastParams()).toEqual({ sort: 'price', dir: 'asc', limit: 50, offset: 0 });
    expect(wrapper.find('.result-count[aria-live="polite"]').text()).toBe('130 Sorten');
    expect(wrapper.findAll('tr.group-row')).toHaveLength(50);
    expect(wrapper.find('tr.group-row a.strain-name').attributes('href')).toBe('/sorte/1');
    expect(wrapper.find('.table-pager-top .pager-range').text()).toBe('1–50 von 130 Sorten');

    const buttons = wrapper.findAll('.table-pager-top .q-pagination button');
    await buttons[buttons.length - 2]!.trigger('click'); // next
    await flushPromises();
    expect(lastParams()).toEqual({ sort: 'price', dir: 'asc', limit: 50, offset: 50 });
    expect(router.currentRoute.value.query).toEqual({ page: '2' });
    expect(wrapper.find('tr.group-row a.strain-name').attributes('href')).toBe('/sorte/51');
  });

  it('sorts on the server via the header buttons and resets the page', async () => {
    await router.replace({ query: { page: '3' } });
    const wrapper = await mountPage();
    expect(lastParams()).toMatchObject({ offset: 100 });
    await wrapper.find('th[data-key="rating"] button').trigger('click');
    await flushPromises();
    expect(lastParams()).toEqual({ sort: 'rating', dir: 'desc', limit: 50, offset: 0 });
    expect(wrapper.find('th[data-key="rating"]').attributes('aria-sort')).toBe('descending');
    expect(router.currentRoute.value.query).toEqual({ sort: 'rating', dir: 'desc' });
  });

  it('searches with a debounce and renders the facets into the filter panel', async () => {
    const wrapper = await mountPage();
    await wrapper.find('.toolbar input').setValue('kush');
    vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS + 1);
    await flushPromises();
    expect(lastParams()).toEqual({ q: 'kush', sort: 'price', dir: 'asc', limit: 50, offset: 0 });

    await wrapper.find('.toggle-filters').trigger('click');
    expect(wrapper.findAll('.chip').map((chip) => chip.text())).toEqual([
      'Hybrid',
      'Indica',
      'Sativa',
    ]);
    await wrapper.find('.chip[data-key="indica"]').trigger('click');
    await flushPromises();
    expect(lastParams()).toMatchObject({ q: 'kush', genetik: ['indica'], offset: 0 });

    await wrapper.find('.toolbar button.clear-button:not(.toggle-filters)').trigger('click');
    await flushPromises();
    expect(lastParams()).toEqual({ sort: 'price', dir: 'asc', limit: 50, offset: 0 });
  });

  it('shows the error state with retry', async () => {
    strainsMock.mockRejectedValueOnce(new Error('boom'));
    const wrapper = await mountPage();
    expect(wrapper.find('.empty .empty-message').text()).toBe(
      'Daten konnten nicht geladen werden.',
    );
    await wrapper.find('.empty button.empty-retry').trigger('click');
    await flushPromises();
    expect(wrapper.findAll('tr.group-row')).toHaveLength(50);
  });
});
