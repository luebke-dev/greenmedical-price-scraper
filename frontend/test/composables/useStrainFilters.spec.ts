import { flushPromises, mount } from '@vue/test-utils';
import type { Pinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, h } from 'vue';
import type { Router } from 'vue-router';
import { getStrains, type StrainsParams } from '@/api/endpoints';
import type { StrainsPage } from '@/api/types';
import { SEARCH_DEBOUNCE_MS, useStrainFilters } from '@/composables/useStrainFilters';
import { useNavigationStore } from '@/stores/navigation';
import { makeFacets, makeListItem, makeStrainsPage } from '../fixtures';
import { createTestPinia, createTestRouter } from '../helpers';

vi.mock('@/api/endpoints', () => ({
  getStrains: vi.fn(),
  getMetadata: vi.fn(),
  getStrain: vi.fn(),
}));

const strainsMock = vi.mocked(getStrains);

type Filters = ReturnType<typeof useStrainFilters>;

/** Facets: price 5.49–8 → slider 5.4–8; thc 20–27; cbd 0.99–5 → 0.9–5. */
function facets() {
  return makeFacets({
    genetik: [
      { value: 'Indica', count: 2 },
      { value: 'Sativa', count: 1 },
    ],
    price: { min: 5.49, max: 8 },
    thc: { min: 20, max: 27 },
    cbd: { min: 0.99, max: 5 },
    rating: null,
  });
}

function pageOf(total = 3): StrainsPage {
  return makeStrainsPage([makeListItem({ id: 1 })], { total, facets: facets() });
}

/** Params of the last GET /strains call. */
function lastParams(): StrainsParams | undefined {
  return strainsMock.mock.calls[strainsMock.mock.calls.length - 1]?.[0];
}

const BASE = { sort: 'price', dir: 'asc', limit: 50, offset: 0 } as const;

describe('useStrainFilters', () => {
  let router: Router;
  let pinia: Pinia;

  async function mountFilters() {
    let filters!: Filters;
    const Host = defineComponent({
      setup() {
        filters = useStrainFilters();
        return () => h('div');
      },
    });
    const wrapper = mount(Host, { global: { plugins: [router, pinia] } });
    await flushPromises();
    return { wrapper, filters };
  }

  beforeEach(async () => {
    vi.useFakeTimers();
    strainsMock.mockReset();
    strainsMock.mockImplementation(() => Promise.resolve(pageOf()));
    pinia = createTestPinia();
    router = createTestRouter();
    await router.push('/');
    await router.isReady();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('requests the first page on mount and derives bounds/chips/full ranges from the facets', async () => {
    const { filters } = await mountFilters();
    expect(strainsMock).toHaveBeenCalledTimes(1);
    expect(lastParams()).toEqual(BASE);
    expect(filters.bounds.value).toEqual({
      price: { min: 5.4, max: 8 },
      thc: { min: 20, max: 27 },
      cbd: { min: 0.9, max: 5 },
    });
    expect(filters.state.ranges).toEqual({
      price: { lo: 5.4, hi: 8 },
      thc: { lo: 20, hi: 27 },
      cbd: { lo: 0.9, hi: 5 },
    });
    expect(filters.genetik.value.map((o) => o.key)).toEqual(['indica', 'sativa']);
    expect(filters.rows.value.map((r) => r.id)).toEqual([1]);
    expect(filters.count.value).toBe(3);
    expect(router.currentRoute.value.query).toEqual({});
  });

  it('turns filter/sort changes into requests, resets to page 1 and mirrors the URL', async () => {
    const { filters } = await mountFilters();
    filters.setPage(3);
    await flushPromises();
    expect(lastParams()).toEqual({ ...BASE, offset: 100 });
    expect(router.currentRoute.value.query).toEqual({ page: '3' });

    filters.toggleGenetik('indica');
    await flushPromises();
    expect(lastParams()).toEqual({ ...BASE, genetik: ['indica'] });
    expect(filters.state.page).toBe(1);
    expect(router.currentRoute.value.query).toEqual({ genetik: ['indica'] });

    filters.setSort('name');
    filters.setRange('price', { lo: 6, hi: 8 });
    await flushPromises();
    expect(lastParams()).toEqual({ ...BASE, genetik: ['indica'], price_min: 6, sort: 'name' });
    expect(router.currentRoute.value.query).toEqual({
      genetik: ['indica'],
      preis: '6-8',
      sort: 'name',
    });

    // Full slider width again → parameter omitted.
    filters.setRange('price', { lo: 5.4, hi: 8 });
    filters.setSort('name');
    await flushPromises();
    expect(lastParams()).toEqual({ ...BASE, genetik: ['indica'], sort: 'name', dir: 'desc' });
    expect(router.currentRoute.value.query).toEqual({
      genetik: ['indica'],
      sort: 'name',
      dir: 'desc',
    });
  });

  it('changes the page size and keeps the first visible row in view', async () => {
    const { filters } = await mountFilters();
    filters.setPage(4); // rows 150–199
    await flushPromises();
    filters.setSize(100);
    await flushPromises();
    expect(filters.state).toMatchObject({ page: 2, size: 100 });
    expect(lastParams()).toEqual({ ...BASE, limit: 100, offset: 100 });
    expect(router.currentRoute.value.query).toEqual({ page: '2', size: '100' });
    filters.setSize(30); // not an allowed size
    expect(filters.state.size).toBe(100);
  });

  it('debounces the search by 250 ms and requests page 1', async () => {
    const { filters } = await mountFilters();
    filters.setPage(2);
    await flushPromises();
    filters.searchInput.value = 'gam';
    await flushPromises();
    expect(filters.state.query).toBe('');
    vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS - 1);
    await flushPromises();
    expect(filters.state.query).toBe('');
    vi.advanceTimersByTime(2);
    await flushPromises();
    expect(filters.state.query).toBe('gam');
    expect(filters.state.page).toBe(1);
    expect(lastParams()).toEqual({ ...BASE, q: 'gam' });
    expect(router.currentRoute.value.query).toEqual({ q: 'gam' });
  });

  it('reads the initial state from the URL, requests it and clamps once the facets arrive', async () => {
    await router.replace({
      query: {
        q: 'a',
        genetik: 'INDICA',
        preis: '0-6',
        sort: 'thc',
        dir: 'desc',
        page: '2',
        size: '25',
      },
    });
    const { filters } = await mountFilters();
    // First request: sent as given (bounds unknown), then clamped to the facets.
    expect(strainsMock.mock.calls[0]?.[0]).toEqual({
      q: 'a',
      genetik: ['indica'],
      price_min: 0,
      price_max: 6,
      sort: 'thc',
      dir: 'desc',
      limit: 25,
      offset: 25,
    });
    expect(filters.state.query).toBe('a');
    expect(filters.searchInput.value).toBe('a');
    expect(filters.state.genetik).toEqual(['indica']);
    expect(filters.state.ranges.price).toEqual({ lo: 5.4, hi: 6 });
    expect(filters.state.sort).toEqual({ key: 'thc', direction: 'desc' });
    expect(filters.state).toMatchObject({ page: 2, size: 25 });
    expect(lastParams()).toEqual({
      q: 'a',
      genetik: ['indica'],
      price_max: 6,
      sort: 'thc',
      dir: 'desc',
      limit: 25,
      offset: 25,
    });
    // The deep link stays as typed: it parses to the same state, so no rewrite is needed.
    expect(router.currentRoute.value.query).toEqual({
      q: 'a',
      genetik: 'INDICA',
      preis: '0-6',
      sort: 'thc',
      dir: 'desc',
      page: '2',
      size: '25',
    });
  });

  it('keeps a deep link intact while the first response is pending and reconciles genetik', async () => {
    let release!: () => void;
    strainsMock.mockImplementationOnce(
      () => new Promise<StrainsPage>((resolve) => (release = () => resolve(pageOf()))),
    );
    await router.replace({ query: { genetik: ['indica', 'ruderalis'], preis: '6-8' } });
    const { filters } = await mountFilters();
    expect(router.currentRoute.value.query).toEqual({
      genetik: ['indica', 'ruderalis'],
      preis: '6-8',
    });
    expect(lastParams()).toEqual({
      ...BASE,
      genetik: ['indica', 'ruderalis'],
      price_min: 6,
      price_max: 8,
    });

    release();
    await flushPromises();
    // Unknown chip dropped, range clamped/normalised; the request key changed → one more call.
    expect(filters.state.genetik).toEqual(['indica']);
    expect(filters.state.ranges.price).toEqual({ lo: 6, hi: 8 });
    expect(router.currentRoute.value.query).toEqual({
      genetik: ['indica', 'ruderalis'],
      preis: '6-8',
    });
    expect(lastParams()).toEqual({ ...BASE, genetik: ['indica'], price_min: 6 });
    // A later change rewrites the URL with the reconciled state.
    filters.setSort('name');
    await flushPromises();
    expect(router.currentRoute.value.query).toEqual({
      genetik: ['indica'],
      preis: '6-8',
      sort: 'name',
    });
  });

  it('follows external navigation (back/forward) and resets filters but not the sort', async () => {
    const { filters } = await mountFilters();
    await router.push({ query: { sort: 'name', dir: 'desc', genetik: 'sativa', page: '2' } });
    await flushPromises();
    expect(filters.state.sort).toEqual({ key: 'name', direction: 'desc' });
    expect(filters.state.genetik).toEqual(['sativa']);
    expect(filters.state.page).toBe(2);
    expect(lastParams()).toEqual({
      ...BASE,
      genetik: ['sativa'],
      sort: 'name',
      dir: 'desc',
      offset: 50,
    });

    filters.reset();
    await flushPromises();
    expect(filters.state.genetik).toEqual([]);
    expect(filters.state.page).toBe(1);
    expect(filters.state.ranges.price).toEqual({ lo: 5.4, hi: 8 });
    expect(filters.state.sort).toEqual({ key: 'name', direction: 'desc' });
    expect(router.currentRoute.value.query).toEqual({ sort: 'name', dir: 'desc' });
    expect(lastParams()).toEqual({ ...BASE, sort: 'name', dir: 'desc' });
  });

  describe('kept-alive overview (index → strain → index)', () => {
    const FILTERED_QUERY = { genetik: ['indica'], sort: 'name', q: 'a', page: '2' };

    async function applyFilters(filters: Filters) {
      filters.toggleGenetik('indica');
      filters.setSort('name');
      filters.searchInput.value = 'a';
      await flushPromises();
      vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS + 1);
      await flushPromises();
      filters.setPage(2);
      await flushPromises();
      expect(router.currentRoute.value.query).toEqual(FILTERED_QUERY);
    }

    function expectFilteredState(filters: Filters) {
      expect(filters.state.genetik).toEqual(['indica']);
      expect(filters.state.sort).toEqual({ key: 'name', direction: 'asc' });
      expect(filters.state.query).toBe('a');
      expect(filters.searchInput.value).toBe('a');
      expect(filters.state.page).toBe(2);
    }

    it('keeps the state while another route is active and restores it on a query-less return', async () => {
      const { filters } = await mountFilters();
      await applyFilters(filters);
      const calls = strainsMock.mock.calls.length;

      await router.push({ name: 'strain', params: { id: 7 } });
      await flushPromises();
      expect(router.currentRoute.value.query).toEqual({});
      expectFilteredState(filters);

      await router.push({ name: 'index' });
      await flushPromises();
      expectFilteredState(filters);
      expect(router.currentRoute.value.query).toEqual(FILTERED_QUERY);
      // Nothing changed → no new request.
      expect(strainsMock).toHaveBeenCalledTimes(calls);
    });

    it('remembers the overview location for the back/brand links', async () => {
      const { filters } = await mountFilters();
      const navigation = useNavigationStore();
      expect(navigation.indexLocation).toEqual({ name: 'index', query: {} });

      await applyFilters(filters);
      expect(navigation.indexLocation).toEqual({ name: 'index', query: FILTERED_QUERY });

      await router.push({ name: 'strain', params: { id: 7 } });
      await flushPromises();
      expect(navigation.indexLocation).toEqual({ name: 'index', query: FILTERED_QUERY });

      await router.push(navigation.indexLocation);
      await flushPromises();
      expectFilteredState(filters);
      expect(router.currentRoute.value.query).toEqual(FILTERED_QUERY);
    });

    it('does not write a debounced search into the URL of another route', async () => {
      const { filters } = await mountFilters();
      filters.searchInput.value = 'gam';
      await router.push({ name: 'strain', params: { id: 7 } });
      vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS + 1);
      await flushPromises();

      expect(filters.state.query).toBe('gam');
      expect(router.currentRoute.value.name).toBe('strain');
      expect(router.currentRoute.value.query).toEqual({});
      expect(useNavigationStore().indexLocation).toEqual({ name: 'index', query: { q: 'gam' } });
      expect(lastParams()).toEqual({ ...BASE, q: 'gam' });

      await router.push({ name: 'index' });
      await flushPromises();
      expect(router.currentRoute.value.query).toEqual({ q: 'gam' });
    });

    it('lets a query in the URL win when returning (history navigation)', async () => {
      const { filters } = await mountFilters();
      await applyFilters(filters);
      await router.push({ name: 'strain', params: { id: 7 } });
      await flushPromises();

      await router.push({ name: 'index', query: { genetik: 'sativa' } });
      await flushPromises();
      expect(filters.state.genetik).toEqual(['sativa']);
      expect(filters.state.query).toBe('');
      expect(filters.searchInput.value).toBe('');
      expect(filters.state.page).toBe(1);
      expect(filters.state.sort).toEqual({ key: 'price', direction: 'asc' });
      expect(lastParams()).toEqual({ ...BASE, genetik: ['sativa'] });
    });

    it('re-derives ranges from facets that changed while another route was active', async () => {
      const { filters } = await mountFilters();
      filters.setRange('price', { lo: 6, hi: 8 });
      await flushPromises();
      expect(router.currentRoute.value.query).toEqual({ preis: '6-8' });

      await router.push({ name: 'strain', params: { id: 7 } });
      await flushPromises();
      // A new run with a narrower price span arrives (e.g. via retry) while away.
      strainsMock.mockImplementation(() =>
        Promise.resolve(
          makeStrainsPage([], {
            facets: makeFacets({ ...facets(), price: { min: 5.49, max: 7 } }),
          }),
        ),
      );
      filters.setSort('price'); // triggers a request while away
      await flushPromises();
      expect(filters.state.ranges.price).toEqual({ lo: 6, hi: 8 });

      await router.push({ name: 'index' });
      await flushPromises();
      expect(filters.bounds.value.price).toEqual({ min: 5.4, max: 7 });
      expect(filters.state.ranges.price).toEqual({ lo: 6, hi: 7 });
      expect(router.currentRoute.value.query).toEqual({ preis: '6-7', sort: 'price', dir: 'desc' });
    });
  });
});
