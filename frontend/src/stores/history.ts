import { defineStore } from 'pinia';
import { shallowReactive } from 'vue';
import { getStrainHistory } from '@/api/endpoints';
import type { History, HistoryBucket } from '@/api/types';

export interface HistoryKey {
  id: number;
  from: string;
  to: string;
  bucket: HistoryBucket;
  pharmacies: boolean;
}

export function historyCacheKey(key: HistoryKey): string {
  return `${key.id}|${key.from}|${key.to}|${key.bucket}|${key.pharmacies ? 1 : 0}`;
}

interface Inflight {
  promise: Promise<History>;
  controller: AbortController;
}

export const useHistoryStore = defineStore('history', () => {
  const cache = shallowReactive(new Map<string, History>());
  const inflight = new Map<string, Inflight>();

  /**
   * Returns the cached history or fetches it. Concurrent callers for the same key share one
   * request. Callers that lose interest simply ignore the result; the response is still cached.
   */
  function fetchHistory(key: HistoryKey): Promise<History> {
    const cacheKey = historyCacheKey(key);
    const cached = cache.get(cacheKey);
    if (cached) return Promise.resolve(cached);

    const pending = inflight.get(cacheKey);
    if (pending) return pending.promise;

    const controller = new AbortController();
    const promise = getStrainHistory(
      key.id,
      {
        from: key.from,
        to: key.to,
        bucket: key.bucket,
        includePartial: true,
        pharmacies: key.pharmacies,
      },
      controller.signal,
    )
      .then((history) => {
        cache.set(cacheKey, history);
        return history;
      })
      .finally(() => {
        inflight.delete(cacheKey);
      });
    inflight.set(cacheKey, { promise, controller });
    return promise;
  }

  /** Aborts every pending request (e.g. when leaving the strain page). */
  function abortAll(): void {
    for (const entry of inflight.values()) entry.controller.abort();
    inflight.clear();
  }

  function clear(): void {
    abortAll();
    cache.clear();
  }

  return { cache, fetchHistory, abortAll, clear };
});
