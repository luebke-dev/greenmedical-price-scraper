import { afterEach, describe, expect, it, vi } from 'vitest';
import { API_BASE, ApiError, buildUrl, fetchJson, isAbortError } from '@/api/client';
import { getRuns, getStrainHistory } from '@/api/endpoints';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

describe('buildUrl', () => {
  it('prefixes the API base and omits null/undefined query values', () => {
    expect(API_BASE).toBe('/api/v1');
    expect(buildUrl('/metadata')).toBe('/api/v1/metadata');
    expect(buildUrl('/strains/7/history', { from: undefined, to: null })).toBe(
      '/api/v1/strains/7/history',
    );
    expect(
      buildUrl('/strains/7/history', {
        from: '2026-07-28T20:00:00.000Z',
        bucket: 'day',
        include_partial: true,
        pharmacies: false,
        limit: 0,
      }),
    ).toBe(
      '/api/v1/strains/7/history?from=2026-07-28T20%3A00%3A00.000Z&bucket=day&include_partial=true&pharmacies=false&limit=0',
    );
  });
});

describe('fetchJson', () => {
  const fetchSpy = vi.spyOn(globalThis, 'fetch');

  afterEach(() => {
    fetchSpy.mockReset();
  });

  it('requests JSON and returns the parsed body', async () => {
    fetchSpy.mockResolvedValue(jsonResponse(200, { status: 'ok' }));
    const controller = new AbortController();
    await expect(
      fetchJson<{ status: string }>('/metadata', { signal: controller.signal }),
    ).resolves.toEqual({ status: 'ok' });
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const [url, init] = fetchSpy.mock.calls[0]!;
    expect(url).toBe('/api/v1/metadata');
    expect(init).toMatchObject({ headers: { Accept: 'application/json' } });
    expect(init?.signal).toBe(controller.signal);
  });

  it('turns a contract error body into an ApiError with code and message', async () => {
    fetchSpy.mockResolvedValue(
      jsonResponse(404, { error: { code: 'not_found', message: 'Sorte 7 nicht gefunden' } }),
    );
    const error = await fetchJson('/strains/7').catch((cause: unknown) => cause);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({
      name: 'ApiError',
      status: 404,
      code: 'not_found',
      message: 'Sorte 7 nicht gefunden',
    });
  });

  it('falls back to http_error for non-JSON or malformed error bodies', async () => {
    fetchSpy.mockResolvedValueOnce(new Response('<html>Bad Gateway</html>', { status: 502 }));
    await expect(fetchJson('/metadata')).rejects.toMatchObject({
      status: 502,
      code: 'http_error',
      message: 'HTTP 502',
    });

    fetchSpy.mockResolvedValueOnce(jsonResponse(500, { error: 'nope' }));
    await expect(fetchJson('/metadata')).rejects.toMatchObject({
      status: 500,
      code: 'http_error',
    });
  });

  it('builds the history and runs queries with contract parameter names', async () => {
    fetchSpy.mockImplementation(() => Promise.resolve(jsonResponse(200, {})));
    await getStrainHistory(7, {
      from: 'A',
      to: 'B',
      bucket: 'run',
      includePartial: true,
      pharmacies: true,
    });
    expect(fetchSpy.mock.calls[0]![0]).toBe(
      '/api/v1/strains/7/history?from=A&to=B&bucket=run&include_partial=true&pharmacies=true',
    );
    await getRuns({ limit: 10, status: 'failed' });
    expect(fetchSpy.mock.calls[1]![0]).toBe('/api/v1/runs?limit=10&status=failed');
  });
});

describe('isAbortError', () => {
  it('recognises DOMException and plain AbortError-shaped objects only', () => {
    expect(isAbortError(new DOMException('x', 'AbortError'))).toBe(true);
    expect(isAbortError({ name: 'AbortError' })).toBe(true);
    expect(isAbortError(new DOMException('x', 'TimeoutError'))).toBe(false);
    expect(isAbortError(new Error('AbortError'))).toBe(false);
    expect(isAbortError(null)).toBe(false);
    expect(isAbortError('AbortError')).toBe(false);
  });
});
