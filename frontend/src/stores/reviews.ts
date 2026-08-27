import { defineStore } from 'pinia';
import { shallowReactive } from 'vue';
import { getReviews } from '@/api/endpoints';
import type { Review, ReviewSort, ReviewSummary, ReviewsResponse } from '@/api/types';

export const REVIEWS_PAGE = 50;
export const DEFAULT_REVIEW_SORT: ReviewSort = 'newest';

export interface ReviewsEntry {
  summary: ReviewSummary;
  reviews: Review[];
  total: number;
}

export function reviewsCacheKey(id: number, sort: ReviewSort): string {
  return `${id}|${sort}`;
}

interface Inflight {
  promise: Promise<ReviewsEntry>;
  controller: AbortController;
}

function toEntry(response: ReviewsResponse): ReviewsEntry {
  return { summary: response.summary, reviews: response.reviews, total: response.total };
}

export const useReviewsStore = defineStore('reviews', () => {
  /** First page (+ appended pages) per strain and sort order. */
  const cache = shallowReactive(new Map<string, ReviewsEntry>());
  const inflight = new Map<string, Inflight>();

  function request(
    cacheKey: string,
    id: number,
    sort: ReviewSort,
    offset: number,
    merge: (response: ReviewsResponse) => ReviewsEntry,
  ): Promise<ReviewsEntry> {
    const pending = inflight.get(cacheKey);
    if (pending) return pending.promise;
    const controller = new AbortController();
    const promise = getReviews(id, { limit: REVIEWS_PAGE, offset, sort }, controller.signal)
      .then((response) => {
        const entry = merge(response);
        cache.set(cacheKey, entry);
        return entry;
      })
      .finally(() => {
        inflight.delete(cacheKey);
      });
    inflight.set(cacheKey, { promise, controller });
    return promise;
  }

  /** Returns the cached entry or fetches the first page. Concurrent callers share one request. */
  function fetchReviews(id: number, sort: ReviewSort = DEFAULT_REVIEW_SORT): Promise<ReviewsEntry> {
    const cacheKey = reviewsCacheKey(id, sort);
    const cached = cache.get(cacheKey);
    if (cached) return Promise.resolve(cached);
    return request(cacheKey, id, sort, 0, toEntry);
  }

  /** Loads the next page after the cached reviews and appends it (no-op when complete). */
  function loadMore(id: number, sort: ReviewSort = DEFAULT_REVIEW_SORT): Promise<ReviewsEntry> {
    const cacheKey = reviewsCacheKey(id, sort);
    const current = cache.get(cacheKey);
    if (!current) return fetchReviews(id, sort);
    if (current.reviews.length >= current.total) return Promise.resolve(current);
    return request(cacheKey, id, sort, current.reviews.length, (response) => {
      const seen = new Set(current.reviews.map((review) => review.id));
      const fresh = response.reviews.filter((review) => !seen.has(review.id));
      return {
        summary: response.summary,
        reviews: [...current.reviews, ...fresh],
        total: response.total,
      };
    });
  }

  function hasMore(id: number, sort: ReviewSort = DEFAULT_REVIEW_SORT): boolean {
    const entry = cache.get(reviewsCacheKey(id, sort));
    return entry ? entry.reviews.length < entry.total : false;
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

  return { cache, fetchReviews, loadMore, hasMore, abortAll, clear };
});
