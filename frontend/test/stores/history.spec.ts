import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getStrainHistory } from '@/api/endpoints';
import type { History } from '@/api/types';
import { historyCacheKey, useHistoryStore, type HistoryKey } from '@/stores/history';
import { makeHistory, makePoint } from '../fixtures';
import { createTestPinia } from '../helpers';

vi.mock('@/api/endpoints', () => ({ getStrainHistory: vi.fn() }));

const fetchMock = vi.mocked(getStrainHistory);

const KEY: HistoryKey = {
  id: 7,
  from: '2026-07-28T20:00:00.000Z',
  to: '2026-08-27T20:00:00.000Z',
  bucket: 'run',
  pharmacies: false,
};

/** A fetch that only settles when the test says so and rejects on abort. */
function deferred() {
  let resolve!: (history: History) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<History>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function abortError(): DOMException {
  return new DOMException('The operation was aborted.', 'AbortError');
}

describe('historyCacheKey', () => {
  it('is built from id|from|to|bucket|pharmacies', () => {
    expect(historyCacheKey(KEY)).toBe('7|2026-07-28T20:00:00.000Z|2026-08-27T20:00:00.000Z|run|0');
    expect(historyCacheKey({ ...KEY, pharmacies: true, bucket: 'day' })).toBe(
      '7|2026-07-28T20:00:00.000Z|2026-08-27T20:00:00.000Z|day|1',
    );
  });
});

describe('useHistoryStore', () => {
  beforeEach(() => {
    createTestPinia();
    fetchMock.mockReset();
  });

  it('fetches with the contract query parameters and caches the response', async () => {
    const history = makeHistory([makePoint()]);
    fetchMock.mockResolvedValue(history);
    const store = useHistoryStore();

    await expect(store.fetchHistory(KEY)).resolves.toBe(history);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0]![0]).toBe(7);
    expect(fetchMock.mock.calls[0]![1]).toEqual({
      from: KEY.from,
      to: KEY.to,
      bucket: 'run',
      includePartial: true,
      pharmacies: false,
    });
    expect(fetchMock.mock.calls[0]![2]).toBeInstanceOf(AbortSignal);

    // Cache hit: no second request, same object.
    await expect(store.fetchHistory({ ...KEY })).resolves.toBe(history);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(store.cache.get(historyCacheKey(KEY))).toBe(history);

    // A different key is a different request.
    fetchMock.mockResolvedValue(makeHistory([]));
    await store.fetchHistory({ ...KEY, pharmacies: true });
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('shares one in-flight request between concurrent callers', async () => {
    const pending = deferred();
    fetchMock.mockReturnValue(pending.promise);
    const store = useHistoryStore();

    const first = store.fetchHistory(KEY);
    const second = store.fetchHistory({ ...KEY });
    expect(fetchMock).toHaveBeenCalledTimes(1);

    const history = makeHistory([makePoint()]);
    pending.resolve(history);
    await expect(first).resolves.toBe(history);
    await expect(second).resolves.toBe(history);

    // Once settled the in-flight entry is gone; the cache answers from now on.
    await expect(store.fetchHistory(KEY)).resolves.toBe(history);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('aborts pending requests and does not cache the failure', async () => {
    const pending = deferred();
    fetchMock.mockImplementation((_id, _params, signal) => {
      signal?.addEventListener('abort', () => pending.reject(abortError()));
      return pending.promise;
    });
    const store = useHistoryStore();

    const request = store.fetchHistory(KEY);
    store.abortAll();
    await expect(request).rejects.toMatchObject({ name: 'AbortError' });
    expect(store.cache.size).toBe(0);

    // A retry after the abort starts a fresh request.
    const history = makeHistory([]);
    fetchMock.mockResolvedValue(history);
    await expect(store.fetchHistory(KEY)).resolves.toBe(history);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('propagates errors and allows a retry', async () => {
    fetchMock.mockRejectedValueOnce(new Error('boom'));
    const store = useHistoryStore();
    await expect(store.fetchHistory(KEY)).rejects.toThrow('boom');
    expect(store.cache.size).toBe(0);

    fetchMock.mockResolvedValue(makeHistory([]));
    await store.fetchHistory(KEY);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('clear() drops the cache', async () => {
    fetchMock.mockResolvedValue(makeHistory([]));
    const store = useHistoryStore();
    await store.fetchHistory(KEY);
    expect(store.cache.size).toBe(1);
    store.clear();
    expect(store.cache.size).toBe(0);
    await store.fetchHistory(KEY);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});
