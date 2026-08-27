import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { deleteSubscription, getSubscription, updateSubscription } from '@/api/endpoints';
import { ApiError } from '@/api/client';
import ManagePage from '@/pages/ManagePage.vue';
import { makeRule, makeSubscription } from '../fixtures';
import { createTestPinia, createTestRouter } from '../helpers';
import { installQuasarPlugin } from '@quasar/quasar-app-extension-testing-unit-vitest';

vi.mock('@/api/endpoints', () => ({
  getStrains: vi.fn(),
  getSubscription: vi.fn(),
  updateSubscription: vi.fn(),
  deleteSubscription: vi.fn(),
}));

installQuasarPlugin();

const loadMock = vi.mocked(getSubscription);
const updateMock = vi.mocked(updateSubscription);
const deleteMock = vi.mocked(deleteSubscription);

async function mountPage(path: string) {
  const router = createTestRouter();
  await router.push(path);
  await router.isReady();
  const wrapper = mount(ManagePage, {
    attachTo: document.body,
    global: { plugins: [router, createTestPinia()] },
  });
  await flushPromises();
  return wrapper;
}

describe('ManagePage', () => {
  beforeEach(() => {
    loadMock.mockReset();
    updateMock.mockReset();
    deleteMock.mockReset();
    document.body.innerHTML = '';
  });

  it('loads the subscription into the editor and saves via PUT', async () => {
    loadMock.mockResolvedValue(makeSubscription());
    updateMock.mockResolvedValue(
      makeSubscription({ rules: [makeRule(), makeRule({ id: 2, kind: 'new_strain' })] }),
    );
    const wrapper = await mountPage('/abo/verwalten?token=m1');
    expect(loadMock).toHaveBeenCalledWith('m1', expect.any(AbortSignal));
    expect(wrapper.text()).toContain('Abo für test@example.de');
    expect(wrapper.findAll('li.rule-row')).toHaveLength(1);

    await wrapper.find('.rule-add').trigger('click');
    await wrapper.findAll('select.field-select')[1]!.setValue('new_strain');
    await wrapper.find('form').trigger('submit');
    await flushPromises();
    expect(updateMock).toHaveBeenCalledWith(
      'm1',
      [{ kind: 'strain_price_below', strain_id: 7, threshold: 6 }, { kind: 'new_strain' }],
      expect.any(AbortSignal),
    );
    expect(wrapper.text()).toContain('Regeln gespeichert.');
    expect(wrapper.findAll('li.rule-row')).toHaveLength(2);
    wrapper.unmount();
  });

  it('unsubscribes after confirming the dialog', async () => {
    loadMock.mockResolvedValue(makeSubscription());
    deleteMock.mockResolvedValue(undefined);
    const wrapper = await mountPage('/abo/verwalten?token=m1');
    await wrapper.find('.danger-button').trigger('click');
    await flushPromises();
    const confirm = Array.from(document.querySelectorAll('button')).find(
      (button) => button.textContent?.trim() === 'Ja, abmelden',
    );
    expect(confirm).toBeDefined();
    confirm!.click();
    await flushPromises();
    expect(deleteMock).toHaveBeenCalledWith('m1', expect.any(AbortSignal));
    expect(wrapper.find('.success').text()).toBe('Du bist abgemeldet');
    expect(wrapper.find('form').exists()).toBe(false);
    wrapper.unmount();
  });

  it('shows "Link ungültig" for an unknown token', async () => {
    loadMock.mockRejectedValue(new ApiError(404, 'not_found', 'x'));
    const wrapper = await mountPage('/abo/verwalten?token=zzz');
    expect(wrapper.find('[role="alert"]').text()).toBe('Link ungültig oder abgelaufen.');
    wrapper.unmount();
  });
});
