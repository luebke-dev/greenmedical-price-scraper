import { flushPromises } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope, ref, type EffectScope } from 'vue';
import { getStrainHistory } from '@/api/endpoints';
import type { History } from '@/api/types';
import { useHistoryQuery } from '@/composables/useHistoryQuery';
import type { HistoryPreset } from '@/lib/history';
import { makeHistory, makePoint } from '../fixtures';
import { createTestPinia } from '../helpers';

vi.mock('@/api/endpoints', () => ({ getStrainHistory: vi.fn() }));

const fetchMock = vi.mocked(getStrainHistory);
const NOW = new Date('2026-08-27T20:00:42Z');

function deferred() {
  let resolve!: (history: History) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<History>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe('useHistoryQuery', () => {
  let scope: EffectScope;

  function setup(id: number | null = 7) {
    const strainId = ref<number | null>(id);
    const preset = ref<HistoryPreset>('30d');
    const pharmacies = ref(false);
    scope = effectScope();
    const query = scope.run(() => useHistoryQuery(strainId, preset, pharmacies, NOW))!;
    return { strainId, preset, pharmacies, ...query };
  }

  beforeEach(() => {
    createTestPinia();
    fetchMock.mockReset();
  });

  it('loads immediately with the preset range and exposes the result', async () => {
    const history = makeHistory([makePoint()]);
    fetchMock.mockResolvedValue(history);
    const query = setup();
    expect(query.loading.value).toBe(true);
    expect(query.range.value).toEqual({
      from: '2026-07-28T20:00:00.000Z',
      to: '2026-08-27T20:00:00.000Z',
      bucket: 'run',
    });
    await flushPromises();
    expect(query.loading.value).toBe(false);
    expect(query.error.value).toBeNull();
    expect(query.history.value).toBe(history);
    expect(fetchMock.mock.calls[0]![1]).toMatchObject({
      from: '2026-07-28T20:00:00.000Z',
      to: '2026-08-27T20:00:00.000Z',
      bucket: 'run',
      pharmacies: false,
    });
    scope.stop();
  });

  it('ignores the response of a superseded request (stale guard)', async () => {
    const first = deferred();
    const second = deferred();
    fetchMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const query = setup();
    await flushPromises();

    query.preset.value = '7d';
    await flushPromises();
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(query.loading.value).toBe(true);

    // The newer request settles first …
    const week = makeHistory([makePoint({ run_id: 2 })]);
    second.resolve(week);
    await flushPromises();
    expect(query.history.value).toBe(week);
    expect(query.loading.value).toBe(false);

    // … and the older one must not overwrite it afterwards.
    first.resolve(makeHistory([makePoint({ run_id: 1 })]));
    await flushPromises();
    expect(query.history.value).toBe(week);
    expect(query.loading.value).toBe(false);
    scope.stop();
  });

  it('keeps the previous history while a new request is in flight', async () => {
    const month = makeHistory([makePoint()]);
    fetchMock.mockResolvedValueOnce(month);
    const pending = deferred();
    fetchMock.mockReturnValueOnce(pending.promise);
    const query = setup();
    await flushPromises();
    expect(query.history.value).toBe(month);

    query.pharmacies.value = true;
    await flushPromises();
    expect(query.loading.value).toBe(true);
    expect(query.history.value).toBe(month);
    expect(fetchMock.mock.calls[1]![1]).toMatchObject({ pharmacies: true });

    const withPharmacies = makeHistory([makePoint()], []);
    pending.resolve(withPharmacies);
    await flushPromises();
    expect(query.history.value).toBe(withPharmacies);
    expect(query.loading.value).toBe(false);
    scope.stop();
  });

  it('maps failures to the German error message and clears the history', async () => {
    fetchMock.mockResolvedValueOnce(makeHistory([makePoint()]));
    fetchMock.mockRejectedValueOnce(new Error('HTTP 500'));
    const query = setup();
    await flushPromises();
    expect(query.history.value).not.toBeNull();

    query.preset.value = '90d';
    await flushPromises();
    expect(query.error.value).toBe('Verlauf konnte nicht geladen werden.');
    expect(query.history.value).toBeNull();
    expect(query.loading.value).toBe(false);

    // reload() recovers.
    const day = makeHistory([makePoint({ at: '2026-08-27', run_count: 4 })], undefined, {
      bucket: 'day',
    });
    fetchMock.mockResolvedValueOnce(day);
    await query.reload();
    expect(query.error.value).toBeNull();
    expect(query.history.value).toBe(day);
    scope.stop();
  });

  it('treats an abort as "no result" rather than an error', async () => {
    fetchMock.mockRejectedValueOnce(new DOMException('aborted', 'AbortError'));
    const query = setup();
    await flushPromises();
    expect(query.error.value).toBeNull();
    expect(query.history.value).toBeNull();
    scope.stop();
  });

  it('does not request anything without a strain id and aborts on dispose', async () => {
    const query = setup(null);
    await flushPromises();
    expect(fetchMock).not.toHaveBeenCalled();
    expect(query.history.value).toBeNull();
    expect(query.loading.value).toBe(false);

    const pending = deferred();
    let aborted = false;
    fetchMock.mockImplementationOnce((_id, _params, signal) => {
      signal?.addEventListener('abort', () => {
        aborted = true;
        pending.reject(new DOMException('aborted', 'AbortError'));
      });
      return pending.promise;
    });
    query.strainId.value = 9;
    await flushPromises();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    scope.stop();
    await flushPromises();
    expect(aborted).toBe(true);
  });
});
