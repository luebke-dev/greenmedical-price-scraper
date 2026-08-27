import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '@/api/client';
import { getMetadata, getStrain, getStrains } from '@/api/endpoints';
import type { StrainDetail } from '@/api/types';
import { catalogErrorMessage, strainErrorMessage, useCatalogStore } from '@/stores/catalog';
import { makeMetadata, makeRun, makeStrain } from '../fixtures';
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
    // The strain exists but there is no usable run: not a "not found".
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

  it('loads metadata and strains once and exposes byId/latestAt', async () => {
    const metadata = makeMetadata();
    metadataMock.mockResolvedValue(metadata);
    strainsMock.mockResolvedValue({
      run: makeRun(),
      reference_run: null,
      strains: [makeStrain({ id: 1 }), makeStrain({ id: 2 })],
    });
    const store = useCatalogStore();
    const first = store.load();
    expect(store.loading).toBe(true);
    await first;
    expect(store.loading).toBe(false);
    expect(store.loaded).toBe(true);
    expect(store.error).toBeNull();
    expect(store.metadata).toEqual(metadata);
    expect(store.strains).toHaveLength(2);
    expect(store.byId.get(2)?.id).toBe(2);
    expect(store.latestAt).toBe('2026-08-27T20:00:00Z');

    await store.load();
    expect(metadataMock).toHaveBeenCalledTimes(1);
    expect(strainsMock).toHaveBeenCalledTimes(1);
  });

  it('records the error message and lets load() retry after a failure', async () => {
    metadataMock.mockRejectedValueOnce(new ApiError(404, 'no_data', 'kein Lauf'));
    strainsMock.mockRejectedValueOnce(new ApiError(404, 'no_data', 'kein Lauf'));
    const store = useCatalogStore();
    await store.load();
    expect(store.loaded).toBe(false);
    expect(store.error).toBe('Noch keine Daten vorhanden.');

    metadataMock.mockResolvedValue(makeMetadata());
    strainsMock.mockResolvedValue({ run: makeRun(), reference_run: null, strains: [] });
    await store.load();
    expect(store.loaded).toBe(true);
    expect(store.error).toBeNull();
  });

  it('caches strain details until invalidated', async () => {
    strainMock.mockResolvedValue(detail(7));
    const store = useCatalogStore();
    const first = await store.loadDetail(7);
    const second = await store.loadDetail(7);
    expect(second).toBe(first);
    expect(strainMock).toHaveBeenCalledTimes(1);

    store.invalidateDetails();
    await store.loadDetail(7);
    expect(strainMock).toHaveBeenCalledTimes(2);
  });
});
