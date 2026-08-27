import { computed, onScopeDispose, reactive, ref, watch } from 'vue';
import { useRoute, useRouter, type LocationQuery } from 'vue-router';
import {
  boundsFromFacets,
  buildStrainsParams,
  fullRanges,
  geneticsFromFacets,
  type RangeKey,
  type RangeValue,
} from '@/lib/filter';
import { toggleSort, type SortKey } from '@/lib/sort';
import {
  defaultFilterState,
  fromQuery,
  isPageSize,
  serializeQuery,
  toQuery,
  type FilterState,
} from '@/lib/url-state';
import { useCatalogStore } from '@/stores/catalog';
import { INDEX_ROUTE_NAME, useNavigationStore } from '@/stores/navigation';

export const SEARCH_DEBOUNCE_MS = 250;

function isEmptyQuery(query: LocationQuery): boolean {
  return Object.keys(query).length === 0;
}

/**
 * Filter/sort/page state for the strain table, mirrored into the URL query
 * (?q&genetics&preis&thc&cbd&sort&dir&page&size) via router.replace and turned into
 * GET /strains requests (catalog.loadPage). The search text is debounced; every filter,
 * search or sort change goes back to page 1.
 *
 * Slider bounds and genetics chips come from the facets of the last response. Before the first
 * response a deep link is sent as given (ranges unclamped, genetics unchecked) and reconciled
 * once the facets arrive.
 *
 * The overview page is kept alive, so the URL is only synchronised while the overview route is
 * active: on other routes the URL belongs to that page and the state is simply preserved.
 */
export function useStrainFilters() {
  const route = useRoute();
  const router = useRouter();
  const navigation = useNavigationStore();
  const catalog = useCatalogStore();

  const bounds = computed(() => boundsFromFacets(catalog.facets));
  const genetics = computed(() => geneticsFromFacets(catalog.facets));
  const geneticsKeys = computed(() => new Set(genetics.value.map((option) => option.key)));

  const state = reactive<FilterState>(defaultFilterState());
  /** Raw text bound to the search input; flows into state.query after the debounce. */
  const searchInput = ref('');

  const isActive = (): boolean => route.name === INDEX_ROUTE_NAME;
  const hasFacets = (): boolean => catalog.facets !== null;

  /**
   * Parse options shared by every URL → state conversion. Unknown genetics keys are only dropped
   * and ranges only clamped once the facets exist – before that every key is unknown and a deep
   * link would lose its filter.
   */
  function parseOptions() {
    const known = hasFacets();
    return {
      bounds: bounds.value,
      geneticsKeys: known ? geneticsKeys.value : undefined,
      passThroughRanges: !known,
    };
  }

  function assign(next: FilterState): void {
    state.query = next.query;
    state.genetics = next.genetics;
    state.ranges = next.ranges;
    state.sort = next.sort;
    state.page = next.page;
    state.size = next.size;
  }

  function currentQuery() {
    return toQuery(state, bounds.value);
  }

  function currentQueryString(): string {
    return serializeQuery(currentQuery());
  }

  /**
   * URL → state. With `force` the state is rebuilt even if the URL did not change – needed
   * when the bounds change, because the ranges must be re-derived from the new bounds.
   */
  function readRoute(force: boolean): void {
    const next = fromQuery(route.query, parseOptions());
    if (!force && serializeQuery(toQuery(next, bounds.value)) === currentQueryString()) return;
    assign(next);
    searchInput.value = next.query;
  }

  /** Re-derives the state from itself, clamping ranges/genetics to the current facets. */
  function rederive(): void {
    assign(fromQuery(currentQuery(), parseOptions()));
  }

  /** State → URL (replace, so filter changes do not pollute the history). */
  function writeRoute(): void {
    if (!isActive()) return;
    const inUrl = serializeQuery(toQuery(fromQuery(route.query, parseOptions()), bounds.value));
    if (inUrl === currentQueryString()) return;
    void router.replace({ query: currentQuery() });
  }

  // (Re-)read the URL whenever the facets (and thereby the bounds/chips) change. While another
  // page is shown the re-derivation is deferred until the overview becomes active again.
  let boundsChangedWhileAway = false;
  // Compared by value: every response carries a fresh facets object, but only a real change
  // (new run) may re-read the URL – a pending router.replace would otherwise be lost.
  const facetsKey = computed(() => JSON.stringify([bounds.value, [...geneticsKeys.value]]));
  watch(
    facetsKey,
    () => {
      if (isActive()) readRoute(true);
      else boundsChangedWhileAway = true;
    },
    { immediate: true },
  );

  // URL → state, only while the overview is the active route. Coming back from another page
  // with an empty query (brand/back link) keeps the preserved state and writes it back into the
  // URL; a query in the URL (history navigation, deep link) wins.
  watch(
    () => [route.name, route.query] as const,
    ([name, query], previous) => {
      if (name !== INDEX_ROUTE_NAME) return;
      const entering = previous?.[0] !== INDEX_ROUTE_NAME;
      const staleBounds = boundsChangedWhileAway;
      boundsChangedWhileAway = false;
      if (entering && isEmptyQuery(query)) {
        if (staleBounds) rederive();
        writeRoute();
        return;
      }
      readRoute(staleBounds);
    },
  );

  // State → URL + remembered overview location. The remembered location follows the state even
  // while another page is shown (e.g. a debounced search that lands after navigating away).
  watch(
    () => currentQueryString(),
    () => {
      navigation.rememberIndex(currentQuery());
      writeRoute();
    },
    { immediate: true },
  );

  // State → request. Only the API-relevant projection triggers a fetch, so e.g. clamping a
  // range to bounds it already satisfies does not refetch.
  const params = computed(() => buildStrainsParams(state, bounds.value));
  const requestKey = computed(() => JSON.stringify(params.value));
  watch(requestKey, () => void catalog.loadPage(params.value), { immediate: true });

  let timer: ReturnType<typeof setTimeout> | null = null;
  watch(searchInput, (value) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      if (state.query === value) return;
      state.query = value;
      state.page = 1;
    }, SEARCH_DEBOUNCE_MS);
  });
  onScopeDispose(() => {
    if (timer) clearTimeout(timer);
  });

  function reset(): void {
    if (timer) clearTimeout(timer);
    timer = null;
    searchInput.value = '';
    state.query = '';
    state.genetics = [];
    state.ranges = fullRanges(bounds.value);
    state.page = 1;
  }

  function toggleGenetics(key: string): void {
    state.genetics = state.genetics.includes(key)
      ? state.genetics.filter((item) => item !== key)
      : [...state.genetics, key];
    state.page = 1;
  }

  function setRange(key: RangeKey, value: RangeValue): void {
    state.ranges = { ...state.ranges, [key]: value };
    state.page = 1;
  }

  function setSort(key: SortKey): void {
    state.sort = toggleSort(state.sort, key);
    state.page = 1;
  }

  function setPage(page: number): void {
    state.page = Math.max(1, Math.floor(page));
  }

  function setSize(size: number): void {
    if (!isPageSize(size) || size === state.size) return;
    // Keep the first visible row in view.
    const firstRow = (state.page - 1) * state.size;
    state.size = size;
    state.page = Math.floor(firstRow / size) + 1;
  }

  return {
    state,
    searchInput,
    bounds,
    genetics,
    params,
    rows: computed(() => catalog.strains),
    count: computed(() => catalog.total),
    reset,
    toggleGenetics,
    setRange,
    setSort,
    setPage,
    setSize,
  };
}
