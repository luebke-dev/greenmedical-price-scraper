import { computed, onScopeDispose, ref, watch, type ComputedRef, type Ref } from 'vue';
import { de } from '@/i18n/de';
import { countdownText, msUntilNextMinute, nextRunTime } from '@/lib/refresh';
import { useCatalogStore } from '@/stores/catalog';

export type RefreshPhase = 'hidden' | 'countdown' | 'running' | 'overdue';

export const POLL_INTERVAL_MS = 30_000;
export const UPDATED_NOTE_MS = 6_000;

export interface RefreshSchedule {
  phase: ComputedRef<RefreshPhase>;
  text: ComputedRef<string>;
  /** True for a few seconds after a new run has landed ("Daten aktualisiert"). */
  updated: Readonly<Ref<boolean>>;
}

/**
 * Drives the refresh banner from `metadata.next_run_at` / `scrape_running`:
 * - the countdown ticks on every full wall-clock minute (and when the tab becomes visible),
 * - while a run is in progress or overdue the metadata is polled every 30 s,
 * - once `run.id` changes the catalog store is told (`markRunChanged`) so the pages reload,
 *   and a transient "Daten aktualisiert" note is shown.
 * All timers and listeners are cleared when the owning scope is disposed.
 */
export function useRefreshSchedule(): RefreshSchedule {
  const catalog = useCatalogStore();
  const now = ref(Date.now());
  const updated = ref(false);

  const nextRunAt = computed(() => catalog.metadata?.next_run_at ?? null);
  const running = computed(() => catalog.metadata?.scrape_running === true);
  const overdue = computed(() => {
    const at = nextRunTime(nextRunAt.value);
    return Number.isFinite(at) && at <= now.value;
  });

  const phase = computed<RefreshPhase>(() => {
    if (running.value) return 'running';
    if (nextRunAt.value === null) return 'hidden';
    return overdue.value ? 'overdue' : 'countdown';
  });

  const text = computed(() => {
    switch (phase.value) {
      case 'running':
        return de.refresh.running;
      case 'overdue':
        return de.refresh.overdue;
      case 'countdown':
        return countdownText(nextRunAt.value ?? '', now.value);
      default:
        return '';
    }
  });

  // --- minute tick, aligned to the wall clock -------------------------------------------
  let tickTimer: ReturnType<typeof setTimeout> | null = null;
  function scheduleTick(): void {
    if (tickTimer) clearTimeout(tickTimer);
    tickTimer = setTimeout(() => {
      tickTimer = null;
      now.value = Date.now();
      scheduleTick();
    }, msUntilNextMinute(Date.now()));
  }
  scheduleTick();

  // --- polling while running/overdue ---------------------------------------------------------
  const polling = computed(() => phase.value === 'running' || phase.value === 'overdue');
  let pollTimer: ReturnType<typeof setTimeout> | null = null;
  function stopPolling(): void {
    if (pollTimer) clearTimeout(pollTimer);
    pollTimer = null;
  }
  function schedulePoll(): void {
    stopPolling();
    pollTimer = setTimeout(() => {
      pollTimer = null;
      void poll();
    }, POLL_INTERVAL_MS);
  }
  async function poll(): Promise<void> {
    await catalog.loadMetadata();
    now.value = Date.now();
    if (polling.value && !pollTimer) schedulePoll();
  }
  watch(
    polling,
    (active) => {
      if (active) schedulePoll();
      else stopPolling();
    },
    { immediate: true },
  );

  // --- run change → pages reload + note --------------------------------------------------
  let noteTimer: ReturnType<typeof setTimeout> | null = null;
  watch(
    () => catalog.metadata?.run.id,
    (id, previous) => {
      if (id === undefined || previous === undefined || id === previous) return;
      catalog.markRunChanged();
      updated.value = true;
      if (noteTimer) clearTimeout(noteTimer);
      noteTimer = setTimeout(() => {
        noteTimer = null;
        updated.value = false;
      }, UPDATED_NOTE_MS);
    },
  );

  // --- visibility: catch up immediately when the tab comes back ------------------------------
  const doc = typeof document === 'undefined' ? null : document;
  function onVisibility(): void {
    if (doc?.visibilityState !== 'visible') return;
    now.value = Date.now();
    scheduleTick();
    if (polling.value) void poll();
  }
  doc?.addEventListener('visibilitychange', onVisibility);

  onScopeDispose(() => {
    if (tickTimer) clearTimeout(tickTimer);
    tickTimer = null;
    stopPolling();
    if (noteTimer) clearTimeout(noteTimer);
    noteTimer = null;
    doc?.removeEventListener('visibilitychange', onVisibility);
  });

  return { phase, text, updated };
}
