import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { confirmSubscription } from '@/api/endpoints';
import { ApiError } from '@/api/client';
import ConfirmPage from '@/pages/ConfirmPage.vue';
import { makeSubscription } from '../fixtures';
import { createTestPinia, createTestRouter } from '../helpers';
import { installQuasarPlugin } from '@quasar/quasar-app-extension-testing-unit-vitest';

vi.mock('@/api/endpoints', () => ({ confirmSubscription: vi.fn() }));

installQuasarPlugin();

const confirmMock = vi.mocked(confirmSubscription);

async function mountPage(path: string) {
  const router = createTestRouter();
  await router.push(path);
  await router.isReady();
  const wrapper = mount(ConfirmPage, { global: { plugins: [router, createTestPinia()] } });
  await flushPromises();
  return wrapper;
}

describe('ConfirmPage', () => {
  beforeEach(() => {
    confirmMock.mockReset();
  });

  it('confirms the token on mount and lists the rules', async () => {
    confirmMock.mockResolvedValue(makeSubscription());
    const wrapper = await mountPage('/abo/bestaetigen?token=abc');
    expect(confirmMock).toHaveBeenCalledWith('abc', expect.any(AbortSignal));
    expect(wrapper.find('.success').text()).toBe('Preisalarm für test@example.de ist aktiv.');
    expect(wrapper.find('.rule-summary-text').text()).toBe(
      'Preis von OG Kush fällt unter 6,00 €/g',
    );
    expect(wrapper.find('a[href="/abo"]').exists()).toBe(true);
  });

  it('shows "Link ungültig" for an unknown token and an error without token', async () => {
    confirmMock.mockRejectedValue(new ApiError(404, 'not_found', 'x'));
    const wrapper = await mountPage('/abo/bestaetigen?token=nope');
    expect(wrapper.find('[role="alert"]').text()).toBe('Link ungültig oder abgelaufen.');

    const noToken = await mountPage('/abo/bestaetigen');
    expect(noToken.find('[role="alert"]').text()).toBe('Kein Bestätigungs-Token in der Adresse.');
    expect(confirmMock).toHaveBeenCalledTimes(1);
  });
});
