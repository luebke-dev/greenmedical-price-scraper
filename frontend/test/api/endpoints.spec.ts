import { afterEach, describe, expect, it, vi } from 'vitest';
import { getOfferHistory, getStrains, queryNumber, strainsQuery } from '@/api/endpoints';

function compact(query: Record<string, string | undefined>): Record<string, string> {
  return Object.fromEntries(
    Object.entries(query).filter((entry): entry is [string, string] => entry[1] !== undefined),
  );
}

describe('strainsQuery', () => {
  it('is empty for the defaults', () => {
    expect(compact(strainsQuery({}))).toEqual({});
    expect(
      compact(
        strainsQuery({ q: '  ', genetik: [], sort: 'price', dir: 'asc', limit: 50, offset: 0 }),
      ),
    ).toEqual({});
  });

  it('joins genetik with commas and rounds numbers to two decimals', () => {
    expect(
      compact(
        strainsQuery({
          q: ' og kush ',
          genetik: ['indica', 'hybrid sativa dominant'],
          price_min: 5.4,
          price_max: 12.345,
          thc_min: 18,
          cbd_max: 1.006,
          rating_min: 4,
          sort: 'rating',
          dir: 'desc',
          limit: 25,
          offset: 50,
        }),
      ),
    ).toEqual({
      q: 'og kush',
      genetik: 'indica,hybrid sativa dominant',
      price_min: '5.4',
      price_max: '12.35',
      thc_min: '18',
      cbd_max: '1.01',
      rating_min: '4',
      sort: 'rating',
      dir: 'desc',
      limit: '25',
      offset: '50',
    });
    expect(queryNumber(undefined)).toBeUndefined();
    expect(queryNumber(Number.NaN)).toBeUndefined();
    expect(queryNumber(0.1 + 0.2)).toBe('0.3');
  });
});

describe('fetching', () => {
  afterEach(() => vi.unstubAllGlobals());

  function stubFetch(body: unknown) {
    const fetchMock = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(
      () => Promise.resolve(new Response(JSON.stringify(body), { status: 200 })),
    );
    vi.stubGlobal('fetch', fetchMock);
    return fetchMock;
  }

  it('getStrains builds the URL per contract', async () => {
    const fetchMock = stubFetch({ strains: [] });
    await getStrains({ genetik: ['indica'], price_min: 6, sort: 'thc', dir: 'desc', offset: 100 });
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      '/api/v1/strains?genetik=indica&price_min=6&sort=thc&dir=desc&offset=100',
    );
  });

  it('getOfferHistory passes range, mode and paging', async () => {
    const fetchMock = stubFetch({ rows: [] });
    await getOfferHistory(7, {
      from: '2026-07-28T20:00:00.000Z',
      to: '2026-08-27T20:00:00.000Z',
      bucket: 'run',
      mode: 'all',
      limit: 25,
      offset: 25,
    });
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      '/api/v1/strains/7/offer-history?from=2026-07-28T20%3A00%3A00.000Z&to=2026-08-27T20%3A00%3A00.000Z&bucket=run&mode=all&limit=25&offset=25',
    );
  });
});
