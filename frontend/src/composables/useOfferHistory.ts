import { computed, onScopeDispose, reactive, ref, shallowRef, watch, type Ref } from 'vue';
import { isAbortError } from '@/api/client';
import { getOfferHistory } from '@/api/endpoints';
import type { OfferHistoryMode, OfferHistoryPage } from '@/api/types';
import { de } from '@/i18n/de';
import {
  DEFAULT_OFFER_HISTORY_MODE,
  DEFAULT_OFFER_HISTORY_SIZE,
  OFFER_HISTORY_SIZES,
  buildOfferHistoryParams,
  presetRange,
  type HistoryPreset,
  type OfferHistoryQueryState,
} from '@/lib/history';

/**
 * One page of the offer history of `strainId` for the chart's preset. Mode/page/size changes
 * request a new page; a request still in flight is aborted. `now` is fixed per instance so the
 * range (and thereby the query) only changes with the preset.
 */
export function useOfferHistory(
  strainId: Ref<number | null>,
  preset: Ref<HistoryPreset>,
  now: Date = new Date(),
) {
  const state = reactive<OfferHistoryQueryState>({
    mode: DEFAULT_OFFER_HISTORY_MODE,
    page: 1,
    size: DEFAULT_OFFER_HISTORY_SIZE,
  });
  const page = shallowRef<OfferHistoryPage | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const range = computed(() => presetRange(preset.value, now));
  const params = computed(() => buildOfferHistoryParams(range.value, state));

  let controller: AbortController | null = null;

  async function load(): Promise<void> {
    controller?.abort();
    const id = strainId.value;
    if (id === null) {
      controller = null;
      page.value = null;
      loading.value = false;
      return;
    }
    const own = new AbortController();
    controller = own;
    loading.value = true;
    error.value = null;
    try {
      const result = await getOfferHistory(id, params.value, own.signal);
      if (own.signal.aborted) return;
      page.value = result;
    } catch (cause) {
      if (own.signal.aborted || isAbortError(cause)) return;
      error.value = de.offerHistory.loadError;
      page.value = null;
    } finally {
      if (controller === own) {
        loading.value = false;
        controller = null;
      }
    }
  }

  // Strain or range change: start over at page 1 (a stale page number would be meaningless).
  watch([strainId, range], () => {
    state.page = 1;
  });
  watch(
    () => JSON.stringify([strainId.value, params.value]),
    () => void load(),
    {
      immediate: true,
    },
  );
  onScopeDispose(() => controller?.abort());

  function setMode(mode: OfferHistoryMode): void {
    if (mode === state.mode) return;
    state.mode = mode;
    state.page = 1;
  }

  function setPage(next: number): void {
    state.page = Math.max(1, Math.floor(next));
  }

  function setSize(size: number): void {
    if (!OFFER_HISTORY_SIZES.includes(size) || size === state.size) return;
    const firstRow = (state.page - 1) * state.size;
    state.size = size;
    state.page = Math.floor(firstRow / size) + 1;
  }

  return { state, page, loading, error, range, params, reload: load, setMode, setPage, setSize };
}
