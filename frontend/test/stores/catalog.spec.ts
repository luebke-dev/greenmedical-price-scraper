import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '@/api/client';
import { getMetadata, getStrain, getStrains } from '@/api/endpoints';
import type { StrainDetail, StrainsPage } from '@/api/types';
import { catalogErrorMessage, strainErrorMessage, useCatalogStore } from '@/stores/catalog';
import { makeListItem, makeMetadata, makeRun, makeStrain, makeStrainsPage } from '../fixtures';
import { createTestPinia } from '../helpers';

vi.mock('@/api/endpoints', () => ({
  getMetadata: vi.fn(),
  getStrains: vi.fn(),
  getStrain: vi.fn(),
}));

const metadataMock = vi.mocked(getMetadata);
const strainsMock = vi.mocked(getStrains);
const strainMock = vi.mocked(getStrain);

function detail(id: number): StrainDetail {
  return {
    ...makeStrain({ id }),
    first_seen_at: '2026-08-01T04:00:00Z',
    last_seen_at: '2026-08-27T20:00:00Z',
    in_latest_run: true,
    run: makeRun(),
  };
}

function abortError(): Error {
  const error = new Error('aborted');
  error.name = 'AbortError';
  return error;
}

/** getStrains stub that resolves with `page` when released and rejects when aborted. */
function deferredStrains(page: StrainsPage) {
  let release!: () => void;
  strainsMock.mockImplementationOnce(
    (_params, signal) =>
      new Promise<StrainsPage>((resolve, reject) => {
        release = () => resolve(page);
        signal?.addEventListener('abort', () => reject(abortError()));
      }),
  );
  return () => release();
}

describe('error messages', () => {
  it('maps API errors of the catalog', () => {
    expect(catalogErrorMessage(new ApiError(404, 'no_data', 'kein Lauf'))).toBe(
      'Noch keine Daten vorhanden.',
    );
    expect(catalogErrorMessage(new ApiError(500, 'internal', 'x'))).toBe(
      'Daten konnten nicht geladen werden.',
    );
    expect(catalogErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'Daten konnten nicht geladen werden.',
    );
  });

  it('maps API errors of a strain detail', () => {
    expect(strainErrorMessage(new ApiError(404, 'not_found', 'x'))).toBe('Sorte nicht gefunden.');
    expect(strainErrorMessage(new ApiError(404, 'http_error', 'HTTP 404'))).toBe(
      'Sorte nicht gefunden.',
    );
    expect(strainErrorMessage(new ApiError(404, 'no_data', 'kein Lauf'))).toBe(
      'Noch keine Daten vorhanden.',
    );
    expect(strainErrorMessage(new ApiError(502, 'http_error', 'HTTP 502'))).toBe(
      'Sorte konnte nicht geladen werden.',
    );
    expect(strainErrorMessage(new Error('x'))).toBe('Sorte konnte nicht geladen werden.');
  });
});

describe('useCatalogStore', () => {
  beforeEach(() => {
    createTestPinia();
    metadataMock.mockReset();
    strainsMock.mockReset();
    strainMock.mockReset();
  });

  it('loads the metadata once and exposes latestAt', async () => {
    const metadata = makeMetadata();
    metadataMock.mockResolvedValue(metadata);
    const store = useCatalogStore();
    await store.load();
    expect(store.metadata).toEqual(metadata);
    expect(store.metadataError).toBeNull();
    expect(store.latestAt).toBe('2026-08-27T20:00:00Z');
    await store.load();
    expect(metadataMock).toHaveBeenCalledTimes(1);
  });

  it('records a metadata error and retries on the next load()', async () => {
    metadataMock.mockRejectedValueOnce(new ApiError(404, 'no_data', 'kein Lauf'));
    const store = useCatalogStore();
    await store.load();
    expect(store.metadata).toBeNull();
    expect(store.metadataError).toBe('Noch keine Daten vorhanden.');
    metadataMock.mockResolvedValue(makeMetadata());
    await store.load();
    expect(store.metadata).not.toBeNull();
    expect(store.metadataError).toBeNull();
  });

  it('loadPage stores the page, total, facets and run', async () => {
    const page = makeStrainsPage([makeListItem({ id: 1 }), makeListItem({ id: 2 })], {
      total: 120,
      limit: 50,
      offset: 50,
    });
    strainsMock.mockResolvedValue(page);
    const store = useCatalogStore();
    const request = store.loadPage({ sort: 'price', dir: 'asc', limit: 50, offset: 50 });
    expect(store.loading).toBe(true);
    await request;
    expect(store.loading).toBe(false);
    expect(store.error).toBeNull();
    expect(store.page).toEqual(page);
    expect(store.strains.map((row) => row.id)).toEqual([1, 2]);
    expect(store.total).toBe(120);
    expect(store.facets).toEqual(page.facets);
    expect(store.run?.id).toBe(40);
    expect(store.referenceRun).toBeNull();
    expect(store.latestAt).toBe('2026-08-27T20:00:00Z');
    expect(strainsMock).toHaveBeenCalledWith(
      { sort: 'price', dir: 'asc', limit: 50, offset: 50 },
      expect.any(AbortSignal),
    );
  });

  it('aborts a superseded request and keeps the result of the latest one', async () => {
    const first = makeStrainsPage([makeListItem({ id: 1 })], { total: 1 });
    const second = makeStrainsPage([makeListItem({ id: 2 })], { total: 1 });
    const releaseFirst = deferredStrains(first);
    const releaseSecond = deferredStrains(second);
    const store = useCatalogStore();

    const a = store.loadPage({ q: 'a' });
    const firstSignal = strainsMock.mock.calls[0]?.[1];
    const b = store.loadPage({ q: 'b' });
    expect(firstSignal?.aborted).toBe(true);
    expect(store.loading).toBe(true);

    releaseSecond();
    await b;
    expect(store.loading).toBe(false);
    expect(store.strains.map((row) => row.id)).toEqual([2]);

    // The first response (or its abort) must neither replace the page nor flag an error.
    releaseFirst();
    await a;
    expect(store.strains.map((row) => row.id)).toEqual([2]);
    expect(store.error).toBeNull();
    expect(store.loading).toBe(false);
  });

  it('records a page error and refresh() repeats the last request', async () => {
    strainsMock.mockRejectedValueOnce(new ApiError(500, 'internal', 'x'));
    const store = useCatalogStore();
    await store.loadPage({ q: 'kush', limit: 25 });
    expect(store.error).toBe('Daten konnten nicht geladen werden.');
    expect(store.page).toBeNull();

    metadataMock.mockResolvedValue(makeMetadata());
    strainsMock.mockResolvedValue(makeStrainsPage([makeListItem({ id: 3 })]));
    await store.refresh();
    expect(store.error).toBeNull();
    expect(store.metadata).not.toBeNull();
    expect(strainsMock).toHaveBeenLastCalledWith({ q: 'kush', limit: 25 }, expect.any(AbortSignal));
    expect(store.strains.map((row) => row.id)).toEqual([3]);
  });

  it('caches strain details until invalidated or until a new run arrives', async () => {
    strainMock.mockResolvedValue(detail(7));
    const store = useCatalogStore();
    const first = await store.loadDetail(7);
    const second = await store.loadDetail(7);
    expect(second).toBe(first);
    expect(strainMock).toHaveBeenCalledTimes(1);

    store.invalidateDetails();
    await store.loadDetail(7);
    expect(strainMock).toHaveBeenCalledTimes(2);

    strainsMock.mockResolvedValueOnce(makeStrainsPage([], { run: makeRun({ id: 40 }) }));
    await store.loadPage({});
    await store.loadDetail(7);
    expect(strainMock).toHaveBeenCalledTimes(2); // same run → cache kept
    strainsMock.mockResolvedValueOnce(makeStrainsPage([], { run: makeRun({ id: 41 }) }));
    await store.loadPage({});
    await store.loadDetail(7);
    expect(strainMock).toHaveBeenCalledTimes(3); // new run → details dropped
  });
});
