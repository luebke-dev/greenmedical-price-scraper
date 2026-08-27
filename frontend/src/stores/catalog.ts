import { defineStore } from 'pinia';
import { computed, ref, shallowRef } from 'vue';
import { ApiError, isAbortError } from '@/api/client';
import { getMetadata, getStrain, getStrains, type StrainsParams } from '@/api/endpoints';
import type { Facets, Metadata, StrainDetail, StrainsPage } from '@/api/types';
import { de } from '@/i18n/de';

export function catalogErrorMessage(error: unknown): string {
  if (error instanceof ApiError && error.code === 'no_data') return de.table.noData;
  return de.table.loadError;
}

export function strainErrorMessage(error: unknown): string {
  if (!(error instanceof ApiError)) return de.strain.loadError;
  // The strain exists but there is no usable run yet (e.g. after a restore).
  if (error.code === 'no_data') return de.table.noData;
  if (error.code === 'not_found' || error.status === 404) return de.strain.notFound;
  return de.strain.loadError;
}

/**
 * Metadata (metric cards) plus the current page of the strain list. The list is filtered,
 * sorted and paginated by the server: `load(params)` fetches one page and cancels a superseded
 * request via AbortController. Strain details are cached per run for the strain page.
 */
export const useCatalogStore = defineStore('catalog', () => {
  const metadata = ref<Metadata | null>(null);
  const metadataError = ref<string | null>(null);
  // Current page (~100 rows max): shallow to avoid deep reactivity cost.
  const page = shallowRef<StrainsPage | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const details = shallowRef(new Map<number, StrainDetail>());
  // Incremented when a new scrape run has landed (see useRefreshSchedule); pages watch it and
  // reload their data for the new run.
  const runChanged = ref(0);

  let metadataInflight: Promise<void> | null = null;
  let controller: AbortController | null = null;
  let lastParams: StrainsParams | null = null;

  const strains = computed(() => page.value?.strains ?? []);
  const total = computed(() => page.value?.total ?? 0);
  const facets = computed<Facets | null>(() => page.value?.facets ?? null);
  const run = computed(() => page.value?.run ?? null);
  const referenceRun = computed(() => page.value?.reference_run ?? null);
  const latestAt = computed(
    () => metadata.value?.generated_at ?? run.value?.finished_at ?? run.value?.started_at ?? null,
  );

  async function loadMetadata(): Promise<void> {
    if (metadataInflight) return metadataInflight;
    metadataError.value = null;
    metadataInflight = (async () => {
      try {
        metadata.value = await getMetadata();
      } catch (cause) {
        if (!isAbortError(cause)) metadataError.value = catalogErrorMessage(cause);
      } finally {
        metadataInflight = null;
      }
    })();
    return metadataInflight;
  }

  /** Loads the metadata once; later calls are no-ops unless the first load failed. */
  async function load(): Promise<void> {
    if (metadata.value) return;
    return loadMetadata();
  }

  /** Fetches one page of the list. A request still in flight is aborted and ignored. */
  async function loadPage(params: StrainsParams): Promise<void> {
    controller?.abort();
    const own = new AbortController();
    controller = own;
    lastParams = { ...params };
    loading.value = true;
    error.value = null;
    try {
      const result = await getStrains(params, own.signal);
      if (own.signal.aborted) return;
      // Cached details belong to the previous run; a fresh list must not pair with stale offers.
      if (page.value && page.value.run.id !== result.run.id) invalidateDetails();
      page.value = result;
    } catch (cause) {
      if (own.signal.aborted || isAbortError(cause)) return;
      error.value = catalogErrorMessage(cause);
    } finally {
      if (controller === own) {
        loading.value = false;
        controller = null;
      }
    }
  }

  /** Retry: metadata (if missing) and the last page request. */
  async function refresh(): Promise<void> {
    const tasks: Promise<void>[] = [];
    if (!metadata.value) tasks.push(loadMetadata());
    if (lastParams) tasks.push(loadPage(lastParams));
    await Promise.all(tasks);
  }

  async function loadDetail(id: number, signal?: AbortSignal): Promise<StrainDetail> {
    const cached = details.value.get(id);
    if (cached) return cached;
    const detail = await getStrain(id, signal);
    const next = new Map(details.value);
    next.set(id, detail);
    details.value = next;
    return detail;
  }

  function invalidateDetails(): void {
    details.value = new Map();
  }

  /** A new run is live: drop the per-run detail cache and tell the pages to reload. */
  function markRunChanged(): void {
    invalidateDetails();
    runChanged.value += 1;
  }

  return {
    metadata,
    metadataError,
    page,
    strains,
    total,
    facets,
    run,
    referenceRun,
    loading,
    error,
    latestAt,
    runChanged,
    load,
    loadMetadata,
    loadPage,
    refresh,
    loadDetail,
    invalidateDetails,
    markRunChanged,
  };
});
