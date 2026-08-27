import { computed, onScopeDispose, reactive, ref, watch, type Ref } from 'vue';
import { useRoute, useRouter, type LocationQuery } from 'vue-router';
import type { Strain } from '@/api/types';
import {
  applyFilters,
  computeAllBounds,
  fullRanges,
  genetikOptions,
  type RangeKey,
  type RangeValue,
} from '@/lib/filter';
import { sortRows, toggleSort, type SortKey } from '@/lib/sort';
import {
  defaultFilterState,
  fromQuery,
  serializeQuery,
  toQuery,
  type FilterState,
} from '@/lib/url-state';
import { INDEX_ROUTE_NAME, useNavigationStore } from '@/stores/navigation';

export const SEARCH_DEBOUNCE_MS = 150;

function isEmptyQuery(query: LocationQuery): boolean {
  return Object.keys(query).length === 0;
}

/**
 * Filter/sort state for the strain table, mirrored into the URL query
 * (?q&genetik&preis&thc&cbd&sort&dir) via router.replace. The search text is debounced.
 *
 * The overview page is kept alive, so the URL is only synchronised while the overview route is
 * active: on other routes the URL belongs to that page and the state is simply preserved.
 */
export function useStrainFilters(rows: Ref<readonly Strain[]>) {
  const route = useRoute();
  const router = useRouter();
  const navigation = useNavigationStore();

  const bounds = computed(() => computeAllBounds(rows.value));
  const genetik = computed(() => genetikOptions(rows.value));
  const genetikKeys = computed(() => new Set(genetik.value.map((option) => option.key)));

  const state = reactive<FilterState>(defaultFilterState());
  /** Raw text bound to the search input; flows into state.query after the debounce. */
  const searchInput = ref('');

  const isActive = (): boolean => route.name === INDEX_ROUTE_NAME;
  const hasData = (): boolean => rows.value.length > 0;

  /**
   * Parse options shared by every URL → state conversion. Unknown genetik keys are only dropped
   * once data exists – before that every key is unknown and a deep link would lose its filter.
   */
  function parseOptions() {
    return { bounds: bounds.value, genetikKeys: hasData() ? genetikKeys.value : undefined };
  }

  function assign(next: FilterState): void {
    state.query = next.query;
    state.genetik = next.genetik;
    state.ranges = next.ranges;
    state.sort = next.sort;
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

  /** Re-derives the state from itself, clamping ranges/genetik to the current data. */
  function rederive(): void {
    assign(fromQuery(currentQuery(), parseOptions()));
  }

  /** State → URL (replace, so filter changes do not pollute the history). */
  function writeRoute(): void {
    // Without data the bounds are empty and every range in the URL would be lost by a rewrite;
    // the URL is re-read once the data (and thereby the bounds) arrives.
    if (!isActive() || !hasData()) return;
    const inUrl = serializeQuery(toQuery(fromQuery(route.query, parseOptions()), bounds.value));
    if (inUrl === currentQueryString()) return;
    void router.replace({ query: currentQuery() });
  }

  // (Re-)read the URL whenever the data (and thereby the bounds) changes. While another page is
  // shown the re-derivation is deferred until the overview becomes active again.
  let boundsChangedWhileAway = false;
  watch(
    bounds,
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

  let timer: ReturnType<typeof setTimeout> | null = null;
  watch(searchInput, (value) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      state.query = value;
    }, SEARCH_DEBOUNCE_MS);
  });
  onScopeDispose(() => {
    if (timer) clearTimeout(timer);
  });

  const filtered = computed(() =>
    sortRows(
      applyFilters(
        rows.value,
        { query: state.query, genetik: new Set(state.genetik), ranges: state.ranges },
        bounds.value,
      ),
      state.sort,
    ),
  );

  function reset(): void {
    if (timer) clearTimeout(timer);
    timer = null;
    searchInput.value = '';
    state.query = '';
    state.genetik = [];
    state.ranges = fullRanges(bounds.value);
  }

  function toggleGenetik(key: string): void {
    state.genetik = state.genetik.includes(key)
      ? state.genetik.filter((item) => item !== key)
      : [...state.genetik, key];
  }

  function setRange(key: RangeKey, value: RangeValue): void {
    state.ranges = { ...state.ranges, [key]: value };
  }

  function setSort(key: SortKey): void {
    state.sort = toggleSort(state.sort, key);
  }

  return {
    state,
    searchInput,
    bounds,
    genetik,
    filtered,
    count: computed(() => filtered.value.length),
    reset,
    toggleGenetik,
    setRange,
    setSort,
  };
}
