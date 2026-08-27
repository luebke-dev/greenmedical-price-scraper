import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import FilterPanel from '@/components/FilterPanel.vue';
import { boundsFromFacets, fullRanges, geneticsFromFacets } from '@/lib/filter';
import { makeFacets } from '../fixtures';
import { installTestPlugins } from '../helpers';

installTestPlugins();

describe('FilterPanel', () => {
  it('renders chips and sliders from the facets', async () => {
    const facets = makeFacets();
    const bounds = boundsFromFacets(facets);
    const wrapper = mount(FilterPanel, {
      props: {
        id: 'filters',
        open: true,
        genetics: geneticsFromFacets(facets),
        selectedGenetics: ['indica'],
        bounds,
        ranges: fullRanges(bounds),
      },
    });
    const chips = wrapper.findAll('.chip');
    // The selected chip carries Quasar's check icon (ligature text).
    expect(chips.map((chip) => chip.text().replace(/^check/, ''))).toEqual([
      'Hybrid',
      'Indica',
      'Sativa',
    ]);
    expect(chips[1]!.classes()).toContain('active');
    await chips[2]!.trigger('click');
    expect(wrapper.emitted('toggleGenetics')).toEqual([['sativa']]);

    const ranges = wrapper.findAll('.filter-range');
    expect(ranges.map((range) => range.attributes('data-key'))).toEqual(['price', 'thc', 'cbd']);
    expect(ranges[0]!.find('.filter-value').text()).toBe('5,40 €/g – 12,40 €/g');
    expect(ranges[1]!.find('.filter-value').text()).toBe('18,0 % – 31,0 %');
    expect(ranges[2]!.find('.filter-value').text()).toBe('0,3 % – 12,0 %');
  });

  it('renders nothing without facets (before the first response)', () => {
    const wrapper = mount(FilterPanel, {
      props: {
        id: 'filters',
        open: true,
        genetics: [],
        selectedGenetics: [],
        bounds: {},
        ranges: {},
      },
    });
    expect(wrapper.findAll('.chip')).toHaveLength(0);
    expect(wrapper.findAll('.filter-range')).toHaveLength(0);
  });

  it('hides sliders whose facet is missing and chips below two options', () => {
    const facets = makeFacets({ genetics: [{ value: 'Indica', count: 9 }], thc: null });
    const bounds = boundsFromFacets(facets);
    const wrapper = mount(FilterPanel, {
      props: {
        id: 'filters',
        open: true,
        genetics: geneticsFromFacets(facets),
        selectedGenetics: [],
        bounds,
        ranges: fullRanges(bounds),
      },
    });
    expect(wrapper.findAll('.chip')).toHaveLength(0);
    expect(wrapper.findAll('.filter-range').map((r) => r.attributes('data-key'))).toEqual([
      'price',
      'cbd',
    ]);
  });
});
