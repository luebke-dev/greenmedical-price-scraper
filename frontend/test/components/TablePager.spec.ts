import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import TablePager from '@/components/TablePager.vue';
import { installTestPlugins } from '../helpers';

installTestPlugins();

describe('TablePager', () => {
  it('shows the visible range, page buttons and the size select', async () => {
    const wrapper = mount(TablePager, {
      props: { page: 3, size: 25, total: 120, noun: 'Bewertungen', sizes: [25, 50, 100] },
    });
    expect(wrapper.find('.pager-range').text()).toBe('51–75 von 120 Bewertungen');
    expect(wrapper.find('.q-pagination').exists()).toBe(true);
    const select = wrapper.find('select');
    expect(select.attributes('aria-label')).toBe('Zeilen pro Seite');
    await select.setValue('50');
    expect(wrapper.emitted('update:size')).toEqual([[50]]);
    const buttons = wrapper.findAll('.q-pagination button');
    await buttons[0]!.trigger('click'); // first page
    expect(wrapper.emitted('update:page')).toEqual([[1]]);
  });

  it('hides the page buttons for a single page and handles an empty result', () => {
    const one = mount(TablePager, { props: { page: 1, size: 50, total: 12, noun: 'Sorten' } });
    expect(one.find('.pager-range').text()).toBe('1–12 von 12 Sorten');
    expect(one.find('.q-pagination').exists()).toBe(false);
    const none = mount(TablePager, { props: { page: 1, size: 50, total: 0, noun: 'Sorten' } });
    expect(none.find('.pager-range').text()).toBe('Keine Sorten');
  });
});
