import { defineStore } from 'pinia';
import { computed, ref, shallowRef } from 'vue';
import { ApiError, isAbortError } from '@/api/client';
import { getMetadata, getStrain, getStrains } from '@/api/endpoints';
import type { Metadata, Run, Strain, StrainDetail } from '@/api/types';
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

export const useCatalogStore = defineStore('catalog', () => {
  const metadata = ref<Metadata | null>(null);
  // ~900 strains with nested offers: shallow to avoid deep reactivity cost.
  const strains = shallowRef<Strain[]>([]);
  const run = ref<Run | null>(null);
  const referenceRun = ref<Run | null>(null);
  const loading = ref(false);
  const loaded = ref(false);
  const error = ref<string | null>(null);
  const details = shallowRef(new Map<number, StrainDetail>());

  let inflight: Promise<void> | null = null;

  const byId = computed(() => new Map(strains.value.map((strain) => [strain.id, strain])));
  const latestAt = computed(
    () => metadata.value?.generated_at ?? run.value?.finished_at ?? run.value?.started_at ?? null,
  );

  async function refresh(): Promise<void> {
    if (inflight) return inflight;
    loading.value = true;
    error.value = null;
    inflight = (async () => {
      try {
        const [meta, list] = await Promise.all([getMetadata(), getStrains()]);
        metadata.value = meta;
        strains.value = list.strains;
        run.value = list.run;
        referenceRun.value = list.reference_run;
        // Cached details belong to the previous run; a fresh list must not pair with stale offers.
        invalidateDetails();
        loaded.value = true;
      } catch (cause) {
        if (!isAbortError(cause)) error.value = catalogErrorMessage(cause);
      } finally {
        loading.value = false;
        inflight = null;
      }
    })();
    return inflight;
  }

  /** Loads once; later calls are no-ops unless the first load failed. */
  async function load(): Promise<void> {
    if (loaded.value) return;
    return refresh();
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

  return {
    metadata,
    strains,
    run,
    referenceRun,
    loading,
    loaded,
    error,
    byId,
    latestAt,
    load,
    refresh,
    loadDetail,
    invalidateDetails,
  };
});
