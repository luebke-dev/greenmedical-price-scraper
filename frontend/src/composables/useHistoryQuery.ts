import { computed, onScopeDispose, ref, shallowRef, watch, type Ref } from 'vue';
import { isAbortError } from '@/api/client';
import type { History } from '@/api/types';
import { de } from '@/i18n/de';
import { presetRange, type HistoryPreset } from '@/lib/history';
import { useHistoryStore } from '@/stores/history';

/**
 * Loads the history for `strainId` whenever the preset or the pharmacy toggle changes.
 * `now` is fixed per composable instance so switching presets back and forth hits the cache.
 * Responses of superseded requests are ignored; pending requests are aborted on dispose.
 */
export function useHistoryQuery(
  strainId: Ref<number | null>,
  preset: Ref<HistoryPreset>,
  pharmacies: Ref<boolean>,
  now: Date = new Date(),
) {
  const store = useHistoryStore();
  const history = shallowRef<History | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const range = computed(() => presetRange(preset.value, now));

  let requestId = 0;

  async function load(): Promise<void> {
    const id = strainId.value;
    const current = ++requestId;
    if (id === null) {
      history.value = null;
      loading.value = false;
      return;
    }
    loading.value = true;
    error.value = null;
    try {
      const result = await store.fetchHistory({ id, ...range.value, pharmacies: pharmacies.value });
      if (current !== requestId) return;
      history.value = result;
    } catch (cause) {
      if (current !== requestId || isAbortError(cause)) return;
      error.value = de.history.loadError;
      history.value = null;
    } finally {
      if (current === requestId) loading.value = false;
    }
  }

  watch([strainId, preset, pharmacies], () => void load(), { immediate: true });
  onScopeDispose(() => {
    requestId++;
    store.abortAll();
  });

  return { history, loading, error, range, reload: load };
}
