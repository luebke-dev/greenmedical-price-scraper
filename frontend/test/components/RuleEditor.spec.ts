import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getStrains } from '@/api/endpoints';
import RuleEditor from '@/components/RuleEditor.vue';
import { MAX_RULES, makeDraft, validateDrafts, type RuleDraft } from '@/lib/rules';
import { makeListItem, makeStrainsPage } from '../fixtures';
import { installTestPlugins } from '../helpers';

vi.mock('@/api/endpoints', () => ({ getStrains: vi.fn() }));

installTestPlugins();

const strainsMock = vi.mocked(getStrains);

function mountEditor(drafts: RuleDraft[], extra: Record<string, unknown> = {}) {
  const wrapper = mount(RuleEditor, {
    props: {
      modelValue: drafts,
      'onUpdate:modelValue': (value: RuleDraft[]) => wrapper.setProps({ modelValue: value }),
      ...extra,
    },
  });
  return wrapper;
}

describe('RuleEditor', () => {
  beforeEach(() => {
    strainsMock.mockReset();
  });

  it('adds and removes rule rows within 1–20', async () => {
    const wrapper = mountEditor([makeDraft()]);
    expect(wrapper.findAll('li.rule-row')).toHaveLength(1);
    expect(wrapper.find('.rule-remove').attributes('disabled')).toBeDefined();

    await wrapper.find('.rule-add').trigger('click');
    await flushPromises();
    expect(wrapper.findAll('li.rule-row')).toHaveLength(2);
    expect(wrapper.find('.rule-remove').attributes('disabled')).toBeUndefined();

    await wrapper.findAll('.rule-remove')[1]!.trigger('click');
    await flushPromises();
    expect(wrapper.findAll('li.rule-row')).toHaveLength(1);

    const full = Array.from({ length: MAX_RULES }, () => makeDraft());
    await wrapper.setProps({ modelValue: full });
    expect(wrapper.find('.rule-add').attributes('disabled')).toBeDefined();
    expect(wrapper.text()).toContain('Höchstens 20 Regeln.');
  });

  it('shows strain and threshold fields depending on the kind', async () => {
    const wrapper = mountEditor([makeDraft({ kind: 'strain_price_below' })]);
    expect(wrapper.find('.rule-field-strain').exists()).toBe(true);
    expect(wrapper.find('.rule-field-threshold').exists()).toBe(true);
    expect(wrapper.find('.rule-field-threshold .q-field__suffix').text()).toBe('€/g');
    expect(wrapper.text()).toContain(
      'Meldet, sobald der günstigste Preis der Sorte unter den Schwellwert fällt.',
    );

    const select = wrapper.find<HTMLSelectElement>('select.field-select');
    expect(select.findAll('option').map((option) => option.text())).toEqual([
      'Sorte wieder verfügbar',
      'Preis einer Sorte unter X €/g',
      'Preis irgendeiner Sorte unter X €/g',
      'Neue Sorte mit mehr als X % THC',
      'Jede neue Sorte',
      'Preisänderung einer Sorte',
    ]);

    await select.setValue('thc_above');
    await flushPromises();
    expect(wrapper.find('.rule-field-strain').exists()).toBe(false);
    expect(wrapper.find('.rule-field-threshold .q-field__suffix').text()).toBe('%');

    await select.setValue('new_strain');
    await flushPromises();
    expect(wrapper.find('.rule-field-strain').exists()).toBe(false);
    expect(wrapper.find('.rule-field-threshold').exists()).toBe(false);

    await select.setValue('strain_available');
    await flushPromises();
    expect(wrapper.find('.rule-field-strain').exists()).toBe(true);
    expect(wrapper.find('.rule-field-threshold').exists()).toBe(false);
  });

  it('emits threshold changes as numbers (comma accepted)', async () => {
    const wrapper = mountEditor([makeDraft({ kind: 'any_price_below' })]);
    const input = wrapper.find('.rule-field-threshold input');
    await input.setValue('5,5');
    await flushPromises();
    expect(wrapper.props('modelValue')[0]!.threshold).toBe(5.5);
    await input.setValue('');
    await flushPromises();
    expect(wrapper.props('modelValue')[0]!.threshold).toBeNull();
  });

  it('renders validation errors per row and for the list', () => {
    const drafts = [
      makeDraft({ kind: 'strain_price_below', strain: null, threshold: null }),
      makeDraft({ kind: 'new_strain' }),
      makeDraft({ kind: 'new_strain' }),
    ];
    const wrapper = mountEditor(drafts, {
      errors: validateDrafts(drafts),
      listError: 'Mindestens eine Regel ist nötig.',
    });
    const alerts = wrapper.findAll('[role="alert"]').map((node) => node.text());
    expect(alerts).toEqual([
      'Mindestens eine Regel ist nötig.',
      'Bitte eine Sorte wählen.',
      'Bitte einen Schwellwert größer 0 eingeben.',
      'Diese Regel ist doppelt.',
    ]);
  });

  it('queries strains by name for the autocomplete (limit 10, sorted by name)', async () => {
    strainsMock.mockResolvedValue(
      makeStrainsPage([makeListItem({ id: 9, name: 'OG Kush', designation: 'OGK 22/1' })]),
    );
    const wrapper = mountEditor([makeDraft({ kind: 'strain_available' })]);
    const select = wrapper.findComponent({ name: 'QSelect' });
    const update = vi.fn((fn: () => void) => fn());
    const abort = vi.fn();
    select.vm.$emit('filter', 'o', update, abort);
    await flushPromises();
    expect(strainsMock).not.toHaveBeenCalled();

    select.vm.$emit('filter', 'og', update, abort);
    await flushPromises();
    expect(strainsMock).toHaveBeenCalledWith(
      { q: 'og', limit: 10, sort: 'name' },
      expect.any(AbortSignal),
    );
    expect(select.props('options')).toEqual([{ id: 9, name: 'OG Kush', designation: 'OGK 22/1' }]);
    expect(abort).not.toHaveBeenCalled();

    select.vm.$emit('update:modelValue', { id: 9, name: 'OG Kush', designation: 'OGK 22/1' });
    await flushPromises();
    expect(wrapper.props('modelValue')[0]!.strain).toEqual({
      id: 9,
      name: 'OG Kush',
      designation: 'OGK 22/1',
    });
    expect(wrapper.find('.rule-field-strain .field-hint').text()).toBe('OGK 22/1');
  });
});
