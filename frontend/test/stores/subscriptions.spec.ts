import { beforeEach, describe, expect, it, vi } from 'vitest';
import { subscriptionError, useSubscriptionsStore } from '@/stores/subscriptions';
import { ApiError } from '@/api/client';
import { makeSubscription } from '../fixtures';
import { createTestPinia } from '../helpers';

const fetchMock = vi.fn<typeof fetch>();

function reply(status: number, payload?: unknown): Response {
  return new Response(payload === undefined ? null : JSON.stringify(payload), {
    status,
    headers: payload === undefined ? {} : { 'Content-Type': 'application/json' },
  });
}

function apiError(status: number, code: string, message: string): Response {
  return reply(status, { error: { code, message } });
}

function lastRequest(): { url: string; init: RequestInit } {
  const call = fetchMock.mock.calls[fetchMock.mock.calls.length - 1]!;
  return { url: call[0] as string, init: call[1] ?? {} };
}

function sentJson(init: RequestInit): unknown {
  return JSON.parse(init.body as string) as unknown;
}

describe('subscriptions store', () => {
  beforeEach(() => {
    createTestPinia();
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
  });

  it('creates a subscription via POST and reports success', async () => {
    fetchMock.mockResolvedValue(reply(202, { status: 'confirmation_sent' }));
    const store = useSubscriptionsStore();
    const payload = { email: 'a@b.de', rules: [{ kind: 'new_strain' as const }], website: '' };
    await expect(store.create(payload)).resolves.toBe(true);
    expect(store.created).toBe(true);
    expect(store.error).toBeNull();
    expect(store.loading).toBe(false);
    const { url, init } = lastRequest();
    expect(url).toBe('/api/v1/subscriptions');
    expect(init.method).toBe('POST');
    expect(init.headers).toMatchObject({ 'Content-Type': 'application/json' });
    expect(sentJson(init)).toEqual(payload);
  });

  it('maps 429 to the rate-limit message and 400 to the backend field message', async () => {
    const store = useSubscriptionsStore();
    fetchMock.mockResolvedValueOnce(apiError(429, 'rate_limited', 'slow down'));
    await expect(store.create({ email: 'a@b.de', rules: [] })).resolves.toBe(false);
    expect(store.created).toBe(false);
    expect(store.error).toEqual({
      kind: 'rate_limit',
      message: 'Zu viele Anfragen – bitte in einer Stunde erneut versuchen.',
    });

    fetchMock.mockResolvedValueOnce(apiError(400, 'bad_request', 'Regel 1: strain_id fehlt'));
    await store.create({ email: 'a@b.de', rules: [] });
    expect(store.error).toEqual({ kind: 'validation', message: 'Regel 1: strain_id fehlt' });

    fetchMock.mockResolvedValueOnce(new Response('nope', { status: 400 }));
    await store.create({ email: 'a@b.de', rules: [] });
    expect(store.error).toEqual({ kind: 'validation', message: 'Eingaben ungültig.' });

    fetchMock.mockRejectedValueOnce(new TypeError('network'));
    await store.create({ email: 'a@b.de', rules: [] });
    expect(store.error).toEqual({
      kind: 'generic',
      message: 'Das hat nicht geklappt. Bitte später erneut versuchen.',
    });
  });

  it('confirms a token and stores the subscription; 404 → invalid link', async () => {
    const subscription = makeSubscription();
    fetchMock.mockResolvedValueOnce(reply(200, subscription));
    const store = useSubscriptionsStore();
    await expect(store.confirm('tok')).resolves.toEqual(subscription);
    expect(store.subscription).toEqual(subscription);
    const { url, init } = lastRequest();
    expect(url).toBe('/api/v1/subscriptions/confirm');
    expect(init.method).toBe('POST');
    expect(sentJson(init)).toEqual({ token: 'tok' });

    fetchMock.mockResolvedValueOnce(apiError(404, 'not_found', 'unknown token'));
    await expect(store.confirm('bad')).resolves.toBeNull();
    expect(store.error).toEqual({
      kind: 'invalid_link',
      message: 'Link ungültig oder abgelaufen.',
    });
  });

  it('loads, updates and deletes via the manage token', async () => {
    const store = useSubscriptionsStore();
    fetchMock.mockResolvedValueOnce(reply(200, makeSubscription()));
    await store.load('m1');
    expect(lastRequest()).toMatchObject({
      url: '/api/v1/subscriptions/manage?token=m1',
      init: { method: 'GET' },
    });
    expect(store.subscription?.email).toBe('test@example.de');

    const updated = makeSubscription({ rules: [] });
    fetchMock.mockResolvedValueOnce(reply(200, updated));
    await expect(store.update('m1', [{ kind: 'new_strain' }])).resolves.toEqual(updated);
    expect(lastRequest()).toMatchObject({
      url: '/api/v1/subscriptions/manage?token=m1',
      init: { method: 'PUT' },
    });
    expect(sentJson(lastRequest().init)).toEqual({
      rules: [{ kind: 'new_strain' }],
    });
    expect(store.subscription).toEqual(updated);

    fetchMock.mockResolvedValueOnce(reply(204));
    await expect(store.remove('m1')).resolves.toBe(true);
    expect(lastRequest()).toMatchObject({
      url: '/api/v1/subscriptions/manage?token=m1',
      init: { method: 'DELETE' },
    });
    expect(store.deleted).toBe(true);
    expect(store.subscription).toBeNull();

    fetchMock.mockResolvedValueOnce(apiError(404, 'not_found', 'gone'));
    await expect(store.remove('m1')).resolves.toBe(false);
    expect(store.error?.kind).toBe('invalid_link');
  });

  it('reset clears every flag', async () => {
    fetchMock.mockResolvedValue(reply(202, { status: 'confirmation_sent' }));
    const store = useSubscriptionsStore();
    await store.create({ email: 'a@b.de', rules: [{ kind: 'new_strain' }] });
    store.reset();
    expect(store.created).toBe(false);
    expect(store.error).toBeNull();
    expect(store.subscription).toBeNull();
  });

  it('subscriptionError maps by status', () => {
    expect(subscriptionError(new ApiError(404, 'not_found', 'x')).kind).toBe('invalid_link');
    expect(subscriptionError(new ApiError(429, 'rate_limited', 'x')).kind).toBe('rate_limit');
    expect(subscriptionError(new ApiError(400, 'bad_request', 'x')).message).toBe('x');
    expect(subscriptionError(new ApiError(500, 'internal', 'x')).kind).toBe('generic');
    expect(subscriptionError(new Error('x')).kind).toBe('generic');
  });
});
