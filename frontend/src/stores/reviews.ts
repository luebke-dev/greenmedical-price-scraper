import { defineStore } from 'pinia';
import { shallowReactive } from 'vue';
import { getReviews } from '@/api/endpoints';
import type { Review, ReviewSort, ReviewSummary, ReviewsResponse } from '@/api/types';

export const REVIEW_PAGE_SIZES: readonly number[] = [25, 50, 100];
export const DEFAULT_REVIEW_PAGE_SIZE = 25;
export const DEFAULT_REVIEW_SORT: ReviewSort = 'newest';

export interface ReviewsQuery {
  sort: ReviewSort;
  limit: number;
  offset: number;
}

export interface ReviewsEntry {
  summary: ReviewSummary;
  reviews: Review[];
  total: number;
  limit: number;
  offset: number;
}

export function reviewsCacheKey(id: number, query: ReviewsQuery): string {
  return `${id}|${query.sort}|${query.limit}|${query.offset}`;
}

interface Inflight {
  promise: Promise<ReviewsEntry>;
  controller: AbortController;
}

function toEntry(response: ReviewsResponse, query: ReviewsQuery): ReviewsEntry {
  return {
    summary: response.summary,
    reviews: response.reviews,
    total: response.total,
    limit: query.limit,
    offset: query.offset,
  };
}

export const useReviewsStore = defineStore('reviews', () => {
  /** One page per strain, sort order, page size and offset. */
  const cache = shallowReactive(new Map<string, ReviewsEntry>());
  const inflight = new Map<string, Inflight>();

  /** Returns the cached page or fetches it. Concurrent callers share one request. */
  function fetchPage(id: number, query: ReviewsQuery): Promise<ReviewsEntry> {
    const cacheKey = reviewsCacheKey(id, query);
    const cached = cache.get(cacheKey);
    if (cached) return Promise.resolve(cached);
    const pending = inflight.get(cacheKey);
    if (pending) return pending.promise;

    const controller = new AbortController();
    const promise = getReviews(
      id,
      { limit: query.limit, offset: query.offset, sort: query.sort },
      controller.signal,
    )
      .then((response) => {
        const entry = toEntry(response, query);
        cache.set(cacheKey, entry);
        return entry;
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

  return { cache, fetchPage, abortAll, clear };
});
