import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Router } from 'vue-router';
import { createSubscription, getStrain } from '@/api/endpoints';
import { ApiError } from '@/api/client';
import SubscribePage from '@/pages/SubscribePage.vue';
import { makeRun, makeStrain } from '../fixtures';
import { createTestPinia, createTestRouter } from '../helpers';
import { installQuasarPlugin } from '@quasar/quasar-app-extension-testing-unit-vitest';

vi.mock('@/api/endpoints', () => ({
  getStrains: vi.fn(),
  getStrain: vi.fn(),
  createSubscription: vi.fn(),
}));

installQuasarPlugin();

const strainMock = vi.mocked(getStrain);
const createMock = vi.mocked(createSubscription);

describe('SubscribePage', () => {
  let router: Router;

  beforeEach(() => {
    strainMock.mockReset();
    createMock.mockReset();
    router = createTestRouter();
  });

  async function mountPage(path: string) {
    await router.push(path);
    await router.isReady();
    const wrapper = mount(SubscribePage, { global: { plugins: [router, createTestPinia()] } });
    await flushPromises();
    return wrapper;
  }

  it('renders the e-mail field, one rule row and the hidden honeypot', async () => {
    const wrapper = await mountPage('/abo');
    expect(wrapper.find('input[type="email"]').exists()).toBe(true);
    expect(wrapper.findAll('li.rule-row')).toHaveLength(1);
    const honeypot = wrapper.find('.hp');
    expect(honeypot.attributes('aria-hidden')).toBe('true');
    const input = honeypot.find('input[name="website"]');
    expect(input.attributes('tabindex')).toBe('-1');
    expect(input.attributes('autocomplete')).toBe('off');
    expect(strainMock).not.toHaveBeenCalled();
  });

  it('prefills "wieder verfügbar" and "Preis unter" for ?strain_id', async () => {
    strainMock.mockResolvedValue({
      ...makeStrain({ id: 7, name: 'OG Kush', designation: 'OGK 22/1', price: 6.49 }),
      first_seen_at: '2026-01-01T00:00:00Z',
      last_seen_at: '2026-08-27T20:00:00Z',
      in_latest_run: true,
      run: makeRun(),
    });
    const wrapper = await mountPage('/abo?strain_id=7');
    expect(strainMock).toHaveBeenCalledWith(7, expect.any(AbortSignal));
    expect(wrapper.text()).toContain('Vorbelegt für OG Kush');
    const selects = wrapper.findAll<HTMLSelectElement>('select.field-select');
    expect(selects.map((select) => select.element.value)).toEqual([
      'strain_available',
      'strain_price_below',
    ]);
    const thresholds = wrapper.findAll<HTMLInputElement>('.rule-field-threshold input');
    expect(thresholds).toHaveLength(1);
    expect(thresholds[0]!.element.value).toBe('6.49');
    expect(wrapper.findAll('.rule-field-strain .field-hint').map((n) => n.text())).toEqual([
      'OGK 22/1',
      'OGK 22/1',
    ]);
  });

  it('validates before sending and shows the success state after submit', async () => {
    createMock.mockResolvedValue({ status: 'confirmation_sent' });
    const wrapper = await mountPage('/abo');
    await wrapper.find('select.field-select').setValue('new_strain');
    await wrapper.find('form').trigger('submit');
    await flushPromises();
    expect(createMock).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain('Bitte eine gültige E-Mail-Adresse eingeben.');

    await wrapper.find('input[type="email"]').setValue('me@example.de');
    await wrapper.find('form').trigger('submit');
    await flushPromises();
    expect(createMock).toHaveBeenCalledWith(
      { email: 'me@example.de', rules: [{ kind: 'new_strain' }], website: '' },
      expect.any(AbortSignal),
    );
    expect(wrapper.find('[role="status"]').text()).toBe(
      'Bestätigungsmail gesendet – bitte Postfach prüfen',
    );
    expect(wrapper.text()).toContain('Spam-Ordner');
    expect(wrapper.find('form').exists()).toBe(false);
  });

  it('shows API errors (rate limit) inline', async () => {
    createMock.mockRejectedValue(new ApiError(429, 'rate_limited', 'x'));
    const wrapper = await mountPage('/abo');
    await wrapper.find('select.field-select').setValue('new_strain');
    await wrapper.find('input[type="email"]').setValue('me@example.de');
    await wrapper.find('form').trigger('submit');
    await flushPromises();
    expect(wrapper.find('.form-error').text()).toBe(
      'Zu viele Anfragen – bitte in einer Stunde erneut versuchen.',
    );
    expect(wrapper.find('form').exists()).toBe(true);
  });
});
