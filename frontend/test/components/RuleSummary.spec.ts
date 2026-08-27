import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import RuleSummary from '@/components/RuleSummary.vue';
import { makeRule } from '../fixtures';

describe('RuleSummary', () => {
  it('lists the kind label and a readable sentence per rule', () => {
    const wrapper = mount(RuleSummary, {
      props: {
        rules: [
          makeRule(),
          { id: 2, kind: 'thc_above', threshold: 22, created_at: '2026-08-27T20:00:00Z' },
        ],
      },
    });
    const items = wrapper.findAll('li');
    expect(items).toHaveLength(2);
    expect(items[0]!.find('.rule-summary-kind').text()).toBe('Preis einer Sorte unter X €/g');
    expect(items[0]!.find('.rule-summary-text').text()).toBe(
      'Preis von OG Kush fällt unter 6,00 €/g',
    );
    expect(items[1]!.find('.rule-summary-text').text()).toBe('Neue Sorte mit mehr als 22,0 % THC');
  });
});
