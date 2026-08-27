import { flushPromises } from '@vue/test-utils';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { RouteLocationNormalized, RouteLocationRaw } from 'vue-router';
import { scrollBehavior } from '@/router/scroll';
import { createTestRouter } from '../helpers';

const TOP = { left: 0, top: 0 };

describe('scrollBehavior (pure)', () => {
  const router = createTestRouter();
  let index: RouteLocationNormalized;
  let filteredIndex: RouteLocationNormalized;
  let strain: RouteLocationNormalized;

  async function visit(location: RouteLocationRaw): Promise<RouteLocationNormalized> {
    await router.push(location);
    return router.currentRoute.value;
  }

  beforeAll(async () => {
    index = await visit({ name: 'index' });
    filteredIndex = await visit({ name: 'index', query: { sort: 'name', genetics: 'indica' } });
    strain = await visit({ name: 'strain', params: { id: 7 } });
  });

  it('restores the saved position on history navigation (strain → back to overview)', () => {
    expect(scrollBehavior(index, strain, { left: 0, top: 900 })).toEqual({ left: 0, top: 900 });
  });

  it('keeps the viewport when only the query changes (filters/sort/search)', () => {
    expect(scrollBehavior(filteredIndex, index, null)).toBe(false);
    expect(scrollBehavior(index, filteredIndex, null)).toBe(false);
  });

  it('starts a different page at the top', () => {
    expect(scrollBehavior(strain, index, null)).toEqual(TOP);
    expect(scrollBehavior(index, strain, null)).toEqual(TOP);
  });
});

describe('scrollBehavior (router integration)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('does not touch the viewport for the query-only replaces of the overview', async () => {
    const router = createTestRouter({ scrollBehavior });
    await router.push('/');
    await router.isReady();
    await flushPromises();
    const scrollTo = vi.spyOn(window, 'scrollTo').mockImplementation(() => undefined);

    // What useStrainFilters does on every filter/sort/search change.
    await router.replace({ query: { sort: 'name' } });
    await flushPromises();
    await router.replace({ query: { sort: 'name', dir: 'desc', genetics: 'indica', q: 'og' } });
    await flushPromises();
    await router.push({ query: {} });
    await flushPromises();
    expect(scrollTo).not.toHaveBeenCalled();

    // A real page change scrolls to the top.
    await router.push({ name: 'strain', params: { id: 7 } });
    await flushPromises();
    expect(scrollTo).toHaveBeenCalledTimes(1);
    expect(scrollTo).toHaveBeenCalledWith(TOP);
  });
});
