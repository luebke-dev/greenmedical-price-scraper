import { defineStore } from 'pinia';
import { ref, shallowRef } from 'vue';
import { ApiError, isAbortError } from '@/api/client';
import {
  confirmSubscription,
  createSubscription,
  deleteSubscription,
  getSubscription,
  updateSubscription,
} from '@/api/endpoints';
import type { RuleInput, Subscription, SubscriptionCreate } from '@/api/types';
import { de } from '@/i18n/de';

export type SubscriptionErrorKind = 'invalid_link' | 'rate_limit' | 'validation' | 'generic';

export interface SubscriptionError {
  kind: SubscriptionErrorKind;
  message: string;
}

/** Maps API failures to the user-facing message (404 → invalid link, 429 → rate limit, 400 → the backend's field message). */
export function subscriptionError(cause: unknown): SubscriptionError {
  const errors = de.subscribe.errors;
  if (cause instanceof ApiError) {
    if (cause.status === 404) return { kind: 'invalid_link', message: errors.invalidLink };
    if (cause.status === 429) return { kind: 'rate_limit', message: errors.rateLimit };
    if (cause.status === 400) {
      const detail = cause.message && cause.code !== 'http_error' ? cause.message : '';
      return { kind: 'validation', message: detail || errors.validation };
    }
  }
  return { kind: 'generic', message: errors.generic };
}

export const useSubscriptionsStore = defineStore('subscriptions', () => {
  const subscription = shallowRef<Subscription | null>(null);
  const loading = ref(false);
  const error = ref<SubscriptionError | null>(null);
  /** POST /subscriptions succeeded (confirmation mail sent). */
  const created = ref(false);
  /** DELETE succeeded (unsubscribed). */
  const deleted = ref(false);

  let controller: AbortController | null = null;

  function begin(): AbortSignal {
    controller?.abort();
    controller = new AbortController();
    loading.value = true;
    error.value = null;
    return controller.signal;
  }

  async function run<T>(signal: AbortSignal, task: () => Promise<T>): Promise<T | null> {
    try {
      const result = await task();
      if (signal.aborted) return null;
      return result;
    } catch (cause) {
      if (signal.aborted || isAbortError(cause)) return null;
      error.value = subscriptionError(cause);
      return null;
    } finally {
      if (!signal.aborted) loading.value = false;
    }
  }

  /** Returns true when the confirmation mail was (reportedly) sent. */
  async function create(payload: SubscriptionCreate): Promise<boolean> {
    const signal = begin();
    created.value = false;
    const result = await run(signal, () => createSubscription(payload, signal));
    created.value = result !== null;
    return created.value;
  }

  async function confirm(token: string): Promise<Subscription | null> {
    const signal = begin();
    const result = await run(signal, () => confirmSubscription(token, signal));
    if (result) subscription.value = result;
    return result;
  }

  async function load(token: string): Promise<Subscription | null> {
    const signal = begin();
    const result = await run(signal, () => getSubscription(token, signal));
    if (result) subscription.value = result;
    return result;
  }

  async function update(token: string, rules: RuleInput[]): Promise<Subscription | null> {
    const signal = begin();
    const result = await run(signal, () => updateSubscription(token, rules, signal));
    if (result) subscription.value = result;
    return result;
  }

  async function remove(token: string): Promise<boolean> {
    const signal = begin();
    deleted.value = false;
    const ok = await run(signal, async () => {
      await deleteSubscription(token, signal);
      return true;
    });
    if (ok) {
      subscription.value = null;
      deleted.value = true;
    }
    return ok === true;
  }

  function reset(): void {
    controller?.abort();
    controller = null;
    subscription.value = null;
    loading.value = false;
    error.value = null;
    created.value = false;
    deleted.value = false;
  }

  return {
    subscription,
    loading,
    error,
    created,
    deleted,
    create,
    confirm,
    load,
    update,
    remove,
    reset,
  };
});
