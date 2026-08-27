import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getReviews } from '@/api/endpoints';
import type { ReviewsResponse } from '@/api/types';
import { REVIEWS_PAGE, reviewsCacheKey, useReviewsStore } from '@/stores/reviews';
import { makeReview, makeReviewsResponse } from '../fixtures';
import { createTestPinia } from '../helpers';

vi.mock('@/api/endpoints', () => ({ getReviews: vi.fn() }));

const fetchMock = vi.mocked(getReviews);

function page(offset: number, size: number, total: number): ReviewsResponse {
  const reviews = Array.from({ length: size }, (_, index) =>
    makeReview({ id: offset + index + 1 }),
  );
  return makeReviewsResponse(reviews, { total });
}

function abortError(): Error {
  const error = new Error('aborted');
  error.name = 'AbortError';
  return error;
}

describe('reviews store', () => {
  beforeEach(() => {
    createTestPinia();
    fetchMock.mockReset();
  });

  it('builds cache keys per strain and sort', () => {
    expect(reviewsCacheKey(7, 'newest')).toBe('7|newest');
    expect(reviewsCacheKey(7, 'lowest')).toBe('7|lowest');
  });

  it('fetches the first page with limit 50 and caches it per strain + sort', async () => {
    fetchMock.mockResolvedValue(page(0, 3, 3));
    const store = useReviewsStore();
    const entry = await store.fetchReviews(7, 'newest');
    expect(entry.reviews).toHaveLength(3);
    expect(fetchMock).toHaveBeenCalledWith(
      7,
      { limit: REVIEWS_PAGE, offset: 0, sort: 'newest' },
      expect.any(AbortSignal),
    );

    await expect(store.fetchReviews(7, 'newest')).resolves.toBe(entry);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    await store.fetchReviews(7, 'highest');
    await store.fetchReviews(8, 'newest');
    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(store.cache.size).toBe(3);
  });

  it('shares one in-flight request between concurrent callers', async () => {
    fetchMock.mockResolvedValue(page(0, 1, 1));
    const store = useReviewsStore();
    const [a, b] = await Promise.all([store.fetchReviews(7), store.fetchReviews(7)]);
    expect(a).toBe(b);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('loadMore appends the next page via offset and de-duplicates ids', async () => {
    fetchMock.mockResolvedValueOnce(page(0, 50, 120));
    const store = useReviewsStore();
    await store.fetchReviews(7);
    expect(store.hasMore(7)).toBe(true);

    fetchMock.mockResolvedValueOnce({
      ...page(50, 50, 120),
      // The backend may shift by a newly scraped review: id 50 shows up again.
      reviews: [makeReview({ id: 50 }), ...page(50, 49, 120).reviews],
    });
    const second = await store.loadMore(7);
    expect(fetchMock).toHaveBeenLastCalledWith(
      7,
      { limit: REVIEWS_PAGE, offset: 50, sort: 'newest' },
      expect.any(AbortSignal),
    );
    expect(second.reviews).toHaveLength(99);
    expect(new Set(second.reviews.map((review) => review.id)).size).toBe(99);
    expect(second.reviews[0]!.id).toBe(1);
    expect(second.reviews[50]!.id).toBe(51);
    expect(store.cache.get(reviewsCacheKey(7, 'newest'))).toBe(second);

    fetchMock.mockResolvedValueOnce(page(99, 21, 120));
    const third = await store.loadMore(7);
    expect(third.reviews).toHaveLength(120);
    expect(store.hasMore(7)).toBe(false);

    // Complete: no further request.
    await expect(store.loadMore(7)).resolves.toBe(third);
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it('loadMore without a cached first page fetches the first page', async () => {
    fetchMock.mockResolvedValue(page(0, 2, 2));
    const store = useReviewsStore();
    await store.loadMore(9, 'oldest');
    expect(fetchMock).toHaveBeenCalledWith(
      9,
      { limit: REVIEWS_PAGE, offset: 0, sort: 'oldest' },
      expect.any(AbortSignal),
    );
  });

  it('abortAll cancels pending requests without caching, clear drops the cache', async () => {
    fetchMock.mockImplementation(
      (_id, _params, signal) =>
        new Promise((_resolve, reject) =>
          signal?.addEventListener('abort', () => reject(abortError())),
        ),
    );
    const store = useReviewsStore();
    const request = store.fetchReviews(7);
    store.abortAll();
    await expect(request).rejects.toMatchObject({ name: 'AbortError' });
    expect(store.cache.size).toBe(0);

    fetchMock.mockResolvedValue(page(0, 1, 1));
    await store.fetchReviews(7);
    expect(store.cache.size).toBe(1);
    store.clear();
    expect(store.cache.size).toBe(0);
  });

  it('propagates errors and allows a retry', async () => {
    fetchMock.mockRejectedValueOnce(new Error('boom'));
    const store = useReviewsStore();
    await expect(store.fetchReviews(7)).rejects.toThrow('boom');
    expect(store.cache.size).toBe(0);
    fetchMock.mockResolvedValue(page(0, 1, 1));
    await store.fetchReviews(7);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});
