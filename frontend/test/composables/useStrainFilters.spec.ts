import { flushPromises, mount } from '@vue/test-utils';
import type { Pinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, h, ref, type Ref } from 'vue';
import type { Router } from 'vue-router';
import type { Strain } from '@/api/types';
import { SEARCH_DEBOUNCE_MS, useStrainFilters } from '@/composables/useStrainFilters';
import { useNavigationStore } from '@/stores/navigation';
import { makeStrain } from '../fixtures';
import { createTestPinia, createTestRouter } from '../helpers';

function rows(): Strain[] {
  return [
    makeStrain({ id: 1, name: 'Alpha', genetik: 'Indica', price: 5.49, thcValue: 27, cbdValue: 1 }),
    makeStrain({ id: 2, name: 'Beta', genetik: 'Sativa', price: 8, thcValue: 20, cbdValue: 0.99 }),
    makeStrain({
      id: 3,
      name: 'Gamma',
      genetik: 'Indica',
      price: null,
      thcValue: null,
      cbdValue: 5,
    }),
  ];
}

type Filters = ReturnType<typeof useStrainFilters>;

describe('useStrainFilters', () => {
  let router: Router;
  let pinia: Pinia;

  async function mountFilters(data: Ref<readonly Strain[]>) {
    let filters!: Filters;
    const Host = defineComponent({
      setup() {
        filters = useStrainFilters(data);
        return () => h('div');
      },
    });
    const wrapper = mount(Host, { global: { plugins: [router, pinia] } });
    await flushPromises();
    return { wrapper, filters };
  }

  beforeEach(async () => {
    vi.useFakeTimers();
    pinia = createTestPinia();
    router = createTestRouter();
    await router.push('/');
    await router.isReady();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('initialises full ranges from the data even when the URL is empty', async () => {
    const data = ref<readonly Strain[]>(rows());
    const { filters } = await mountFilters(data);
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
    expect(filters.filtered.value.map((r) => r.id)).toEqual([1, 2, 3]);
    expect(filters.count.value).toBe(3);
  });

  it('re-derives ranges when the data (bounds) changes', async () => {
    const data = ref<readonly Strain[]>([]);
    const { filters } = await mountFilters(data);
    expect(filters.state.ranges).toEqual({});
    data.value = rows();
    await flushPromises();
    expect(filters.state.ranges.price).toEqual({ lo: 5.4, hi: 8 });
  });

  it('writes state changes to the URL with router.replace', async () => {
    const data = ref<readonly Strain[]>(rows());
    const { filters } = await mountFilters(data);
    filters.toggleGenetik('indica');
    await flushPromises();
    expect(router.currentRoute.value.query).toEqual({ genetik: ['indica'] });
    expect(filters.filtered.value.map((r) => r.id)).toEqual([1, 3]);

    filters.setSort('name');
    filters.setRange('price', { lo: 6, hi: 8 });
    await flushPromises();
    expect(router.currentRoute.value.query).toEqual({
      genetik: ['indica'],
      preis: '6-8',
      sort: 'name',
    });
    // Narrowed price range hides the null-price row; sorted by name.
    expect(filters.filtered.value.map((r) => r.id)).toEqual([]);
    filters.toggleGenetik('indica');
    await flushPromises();
    expect(filters.filtered.value.map((r) => r.id)).toEqual([2]);
    expect(router.currentRoute.value.query).toEqual({ preis: '6-8', sort: 'name' });
  });

  it('debounces the search input', async () => {
    const data = ref<readonly Strain[]>(rows());
    const { filters } = await mountFilters(data);
    filters.searchInput.value = 'gam';
    await flushPromises();
    expect(filters.state.query).toBe('');
    vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS + 1);
    await flushPromises();
    expect(filters.state.query).toBe('gam');
    expect(filters.filtered.value.map((r) => r.id)).toEqual([3]);
    expect(router.currentRoute.value.query).toEqual({ q: 'gam' });
  });

  it('reads the initial state from the URL and clamps it', async () => {
    await router.replace({
      query: { q: 'a', genetik: 'INDICA', preis: '0-6', sort: 'thc', dir: 'desc' },
    });
    const data = ref<readonly Strain[]>(rows());
    const { filters } = await mountFilters(data);
    expect(filters.state.query).toBe('a');
    expect(filters.searchInput.value).toBe('a');
    expect(filters.state.genetik).toEqual(['indica']);
    expect(filters.state.ranges.price).toEqual({ lo: 5.4, hi: 6 });
    expect(filters.state.sort).toEqual({ key: 'thc', direction: 'desc' });
    expect(filters.filtered.value.map((r) => r.id)).toEqual([1]);
  });

  it('keeps a deep link intact when it is opened before the data has loaded', async () => {
    await router.replace({ query: { genetik: 'indica', preis: '6-8' } });
    const data = ref<readonly Strain[]>([]);
    const { filters } = await mountFilters(data);
    // No rewrite while the catalog is still empty.
    expect(router.currentRoute.value.query).toEqual({ genetik: 'indica', preis: '6-8' });

    data.value = rows();
    await flushPromises();
    expect(router.currentRoute.value.query).toEqual({ genetik: 'indica', preis: '6-8' });
    expect(filters.state.genetik).toEqual(['indica']);
    expect(filters.state.ranges.price).toEqual({ lo: 6, hi: 8 });
    expect(filters.filtered.value.map((r) => r.id)).toEqual([]);
  });

  it('follows external navigation (back/forward) and resets filters but not the sort', async () => {
    const data = ref<readonly Strain[]>(rows());
    const { filters } = await mountFilters(data);
    await router.push({ query: { sort: 'name', dir: 'desc', genetik: 'sativa' } });
    await flushPromises();
    expect(filters.state.sort).toEqual({ key: 'name', direction: 'desc' });
    expect(filters.state.genetik).toEqual(['sativa']);

    filters.reset();
    await flushPromises();
    expect(filters.state.genetik).toEqual([]);
    expect(filters.state.ranges.price).toEqual({ lo: 5.4, hi: 8 });
    expect(filters.state.sort).toEqual({ key: 'name', direction: 'desc' });
    expect(router.currentRoute.value.query).toEqual({ sort: 'name', dir: 'desc' });
  });

  describe('kept-alive overview (index → strain → index)', () => {
    const FILTERED_QUERY = { genetik: ['indica'], sort: 'name', q: 'a' };

    async function applyFilters(filters: Filters) {
      filters.toggleGenetik('indica');
      filters.setSort('name');
      filters.searchInput.value = 'a';
      await flushPromises();
      vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS + 1);
      await flushPromises();
      expect(router.currentRoute.value.query).toEqual(FILTERED_QUERY);
      expect(filters.filtered.value.map((r) => r.id)).toEqual([1, 3]);
    }

    function expectFilteredState(filters: Filters) {
      expect(filters.state.genetik).toEqual(['indica']);
      expect(filters.state.sort).toEqual({ key: 'name', direction: 'asc' });
      expect(filters.state.query).toBe('a');
      expect(filters.searchInput.value).toBe('a');
      expect(filters.filtered.value.map((r) => r.id)).toEqual([1, 3]);
    }

    it('keeps the state while another route is active and restores it on a query-less return', async () => {
      const data = ref<readonly Strain[]>(rows());
      const { filters } = await mountFilters(data);
      await applyFilters(filters);

      await router.push({ name: 'strain', params: { id: 7 } });
      await flushPromises();
      // The strain page's URL is not touched and the state survives the route change.
      expect(router.currentRoute.value.query).toEqual({});
      expectFilteredState(filters);

      // "← Zur Übersicht" / brand without a query: state wins and is written back to the URL.
      await router.push({ name: 'index' });
      await flushPromises();
      expectFilteredState(filters);
      expect(router.currentRoute.value.query).toEqual(FILTERED_QUERY);
    });

    it('remembers the overview location for the back/brand links', async () => {
      const data = ref<readonly Strain[]>(rows());
      const { filters } = await mountFilters(data);
      const navigation = useNavigationStore();
      expect(navigation.indexLocation).toEqual({ name: 'index', query: {} });

      await applyFilters(filters);
      expect(navigation.indexLocation).toEqual({ name: 'index', query: FILTERED_QUERY });

      await router.push({ name: 'strain', params: { id: 7 } });
      await flushPromises();
      expect(navigation.indexLocation).toEqual({ name: 'index', query: FILTERED_QUERY });

      // Following the remembered link reproduces the state without any rewrite.
      await router.push(navigation.indexLocation);
      await flushPromises();
      expectFilteredState(filters);
      expect(router.currentRoute.value.query).toEqual(FILTERED_QUERY);
    });

    it('does not write a debounced search into the URL of another route', async () => {
      const data = ref<readonly Strain[]>(rows());
      const { filters } = await mountFilters(data);
      filters.searchInput.value = 'gam';
      await router.push({ name: 'strain', params: { id: 7 } });
      vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS + 1);
      await flushPromises();

      expect(filters.state.query).toBe('gam');
      expect(router.currentRoute.value.name).toBe('strain');
      expect(router.currentRoute.value.query).toEqual({});
      // …but the remembered overview location already carries it.
      expect(useNavigationStore().indexLocation).toEqual({ name: 'index', query: { q: 'gam' } });

      await router.push({ name: 'index' });
      await flushPromises();
      expect(router.currentRoute.value.query).toEqual({ q: 'gam' });
      expect(filters.filtered.value.map((r) => r.id)).toEqual([3]);
    });

    it('lets a query in the URL win when returning (history navigation)', async () => {
      const data = ref<readonly Strain[]>(rows());
      const { filters } = await mountFilters(data);
      await applyFilters(filters);
      await router.push({ name: 'strain', params: { id: 7 } });
      await flushPromises();

      await router.push({ name: 'index', query: { genetik: 'sativa' } });
      await flushPromises();
      expect(filters.state.genetik).toEqual(['sativa']);
      expect(filters.state.query).toBe('');
      expect(filters.searchInput.value).toBe('');
      expect(filters.state.sort).toEqual({ key: 'price', direction: 'asc' });
      expect(filters.filtered.value.map((r) => r.id)).toEqual([2]);
    });

    it('re-derives ranges from bounds that changed while another route was active', async () => {
      const data = ref<readonly Strain[]>(rows());
      const { filters } = await mountFilters(data);
      filters.setRange('price', { lo: 6, hi: 8 });
      await flushPromises();
      expect(router.currentRoute.value.query).toEqual({ preis: '6-8' });

      await router.push({ name: 'strain', params: { id: 7 } });
      await flushPromises();
      // New data with a narrower price span: the stored range must be clamped on return.
      data.value = [
        makeStrain({ id: 1, name: 'Alpha', price: 5.49 }),
        makeStrain({ id: 2, name: 'Beta', price: 7 }),
      ];
      await flushPromises();
      expect(filters.state.ranges.price).toEqual({ lo: 6, hi: 8 });

      await router.push({ name: 'index' });
      await flushPromises();
      expect(filters.bounds.value.price).toEqual({ min: 5.4, max: 7 });
      expect(filters.state.ranges.price).toEqual({ lo: 6, hi: 7 });
      expect(router.currentRoute.value.query).toEqual({ preis: '6-7' });
      expect(filters.filtered.value.map((r) => r.id)).toEqual([2]);
    });
  });
});
