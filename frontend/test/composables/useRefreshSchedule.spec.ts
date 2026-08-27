import { flushPromises } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope, type EffectScope } from 'vue';
import { getMetadata } from '@/api/endpoints';
import type { Metadata } from '@/api/types';
import { POLL_INTERVAL_MS, useRefreshSchedule } from '@/composables/useRefreshSchedule';
import { useCatalogStore } from '@/stores/catalog';
import { makeMetadata, makeRun } from '../fixtures';
import { createTestPinia } from '../helpers';

vi.mock('@/api/endpoints', () => ({
  getMetadata: vi.fn(),
  getStrains: vi.fn(),
  getStrain: vi.fn(),
}));

const metadataMock = vi.mocked(getMetadata);
// 20:00:42 – not aligned to the minute on purpose.
const NOW = new Date('2026-08-27T20:00:42Z');

describe('useRefreshSchedule', () => {
  let scope: EffectScope;
  let catalog: ReturnType<typeof useCatalogStore>;

  function setup(metadata: Metadata | null = makeMetadata()) {
    catalog = useCatalogStore();
    catalog.metadata = metadata;
    scope = effectScope();
    return scope.run(() => useRefreshSchedule())!;
  }

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
    createTestPinia();
    metadataMock.mockReset();
  });

  afterEach(() => {
    scope.stop();
    vi.useRealTimers();
  });

  it('is hidden without metadata or without a schedule', () => {
    const schedule = setup(null);
    expect(schedule.phase.value).toBe('hidden');
    expect(schedule.text.value).toBe('');
    catalog.metadata = makeMetadata({ next_run_at: null });
    expect(schedule.phase.value).toBe('hidden');
  });

  it('shows the countdown and ticks on the full minute', () => {
    const schedule = setup(makeMetadata({ next_run_at: '2026-08-27T21:00:00Z' }));
    expect(schedule.phase.value).toBe('countdown');
    expect(schedule.text.value).toBe('Nächste Aktualisierung in 1 Std.');
    vi.advanceTimersByTime(17_999);
    expect(schedule.text.value).toBe('Nächste Aktualisierung in 1 Std.');
    vi.advanceTimersByTime(1);
    expect(schedule.text.value).toBe('Nächste Aktualisierung in 59 Minuten');
    vi.advanceTimersByTime(60_000);
    expect(schedule.text.value).toBe('Nächste Aktualisierung in 58 Minuten');
    expect(metadataMock).not.toHaveBeenCalled();
  });

  it('switches to running and polls every 30 s until the run id changes', async () => {
    const schedule = setup(makeMetadata({ run: makeRun({ id: 10 }) }));
    expect(catalog.runChanged).toBe(0);

    catalog.metadata = makeMetadata({ scrape_running: true, run: makeRun({ id: 10 }) });
    await flushPromises();
    expect(schedule.phase.value).toBe('running');
    expect(schedule.text.value).toBe('Aktualisierung läuft …');

    metadataMock.mockResolvedValue(
      makeMetadata({ scrape_running: true, run: makeRun({ id: 10 }) }),
    );
    await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS);
    expect(metadataMock).toHaveBeenCalledTimes(1);
    expect(catalog.runChanged).toBe(0);
    expect(schedule.updated.value).toBe(false);

    metadataMock.mockResolvedValue(
      makeMetadata({
        scrape_running: false,
        run: makeRun({ id: 11 }),
        next_run_at: '2026-08-27T22:00:00Z',
      }),
    );
    await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS);
    expect(metadataMock).toHaveBeenCalledTimes(2);
    expect(catalog.runChanged).toBe(1);
    expect(schedule.updated.value).toBe(true);
    expect(schedule.phase.value).toBe('countdown');

    // Polling stopped, the note disappears after 6 s.
    await vi.advanceTimersByTimeAsync(6_000);
    expect(schedule.updated.value).toBe(false);
    await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 2);
    expect(metadataMock).toHaveBeenCalledTimes(2);
  });

  it('treats a next_run_at in the past as overdue and polls', async () => {
    const schedule = setup(makeMetadata({ next_run_at: '2026-08-27T20:00:00Z' }));
    expect(schedule.phase.value).toBe('overdue');
    expect(schedule.text.value).toBe('Aktualisierung steht an …');
    metadataMock.mockResolvedValue(makeMetadata({ next_run_at: '2026-08-27T20:00:00Z' }));
    await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS);
    expect(metadataMock).toHaveBeenCalledTimes(1);
  });

  it('polls immediately when the tab becomes visible while running', async () => {
    setup(makeMetadata({ scrape_running: true }));
    metadataMock.mockResolvedValue(makeMetadata({ scrape_running: true }));
    document.dispatchEvent(new Event('visibilitychange'));
    await flushPromises();
    expect(metadataMock).toHaveBeenCalledTimes(1);
  });

  it('clears timers and listeners on dispose', async () => {
    const schedule = setup(makeMetadata({ scrape_running: true }));
    metadataMock.mockResolvedValue(makeMetadata({ scrape_running: true }));
    scope.stop();
    await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 3);
    expect(metadataMock).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
    document.dispatchEvent(new Event('visibilitychange'));
    await flushPromises();
    expect(metadataMock).not.toHaveBeenCalled();
    expect(schedule.phase.value).toBe('running');
  });
});
