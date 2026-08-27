import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getReviews } from '@/api/endpoints';
import type { ReviewsResponse } from '@/api/types';
import { DEFAULT_REVIEW_PAGE_SIZE, reviewsCacheKey, useReviewsStore } from '@/stores/reviews';
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

const FIRST = { sort: 'newest', limit: DEFAULT_REVIEW_PAGE_SIZE, offset: 0 } as const;

describe('reviews store', () => {
  beforeEach(() => {
    createTestPinia();
    fetchMock.mockReset();
  });

  it('defaults to 25 per page and builds cache keys per strain, sort, limit and offset', () => {
    expect(DEFAULT_REVIEW_PAGE_SIZE).toBe(25);
    expect(reviewsCacheKey(7, FIRST)).toBe('7|newest|25|0');
    expect(reviewsCacheKey(7, { sort: 'lowest', limit: 100, offset: 200 })).toBe(
      '7|lowest|100|200',
    );
  });

  it('fetches a page with limit/offset/sort and caches it per key', async () => {
    fetchMock.mockResolvedValue(page(0, 3, 3));
    const store = useReviewsStore();
    const entry = await store.fetchPage(7, FIRST);
    expect(entry).toMatchObject({ total: 3, limit: 25, offset: 0 });
    expect(entry.reviews).toHaveLength(3);
    expect(fetchMock).toHaveBeenCalledWith(
      7,
      { limit: 25, offset: 0, sort: 'newest' },
      expect.any(AbortSignal),
    );

    await expect(store.fetchPage(7, FIRST)).resolves.toBe(entry);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    await store.fetchPage(7, { ...FIRST, offset: 25 });
    await store.fetchPage(7, { ...FIRST, sort: 'highest' });
    await store.fetchPage(7, { ...FIRST, limit: 100 });
    await store.fetchPage(8, FIRST);
    expect(fetchMock).toHaveBeenCalledTimes(5);
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      7,
      { limit: 25, offset: 25, sort: 'newest' },
      expect.any(AbortSignal),
    );
    expect(store.cache.size).toBe(5);
  });

  it('shares one in-flight request between concurrent callers', async () => {
    fetchMock.mockResolvedValue(page(0, 1, 1));
    const store = useReviewsStore();
    const [a, b] = await Promise.all([store.fetchPage(7, FIRST), store.fetchPage(7, FIRST)]);
    expect(a).toBe(b);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('abortAll cancels pending requests without caching, clear drops the cache', async () => {
    fetchMock.mockImplementation(
      (_id, _params, signal) =>
        new Promise((_resolve, reject) =>
          signal?.addEventListener('abort', () => reject(abortError())),
        ),
    );
    const store = useReviewsStore();
    const request = store.fetchPage(7, FIRST);
    store.abortAll();
    await expect(request).rejects.toMatchObject({ name: 'AbortError' });
    expect(store.cache.size).toBe(0);

    fetchMock.mockResolvedValue(page(0, 1, 1));
    await store.fetchPage(7, FIRST);
    expect(store.cache.size).toBe(1);
    store.clear();
    expect(store.cache.size).toBe(0);
  });

  it('propagates errors and allows a retry', async () => {
    fetchMock.mockRejectedValueOnce(new Error('boom'));
    const store = useReviewsStore();
    await expect(store.fetchPage(7, FIRST)).rejects.toThrow('boom');
    expect(store.cache.size).toBe(0);
    fetchMock.mockResolvedValue(page(0, 1, 1));
    await store.fetchPage(7, FIRST);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});
