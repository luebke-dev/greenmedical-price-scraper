import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import type { LocationQueryRaw, RouteLocationRaw } from 'vue-router';

export const INDEX_ROUTE_NAME = 'index';

/**
 * Remembers the query (filters, sort, search) of the overview page. The overview is kept alive,
 * so links back to it ("Zur Übersicht", brand) must carry that query – otherwise the URL and the
 * preserved component state would drift apart.
 */
export const useNavigationStore = defineStore('navigation', () => {
  const indexQuery = ref<LocationQueryRaw>({});

  const indexLocation = computed<RouteLocationRaw>(() => ({
    name: INDEX_ROUTE_NAME,
    query: { ...indexQuery.value },
  }));

  function rememberIndex(query: LocationQueryRaw): void {
    indexQuery.value = { ...query };
  }

  return { indexQuery, indexLocation, rememberIndex };
});
