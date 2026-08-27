import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import RatingStars from '@/components/RatingStars.vue';

function classesOf(value: number): string[] {
  const wrapper = mount(RatingStars, { props: { value } });
  return wrapper
    .findAll('.star')
    .map((star) => (star.classes('full') ? 'full' : star.classes('half') ? 'half' : 'empty'));
}

describe('RatingStars', () => {
  it('renders 5 stars with full/half/empty fills', () => {
    expect(classesOf(4)).toEqual(['full', 'full', 'full', 'full', 'empty']);
    expect(classesOf(3.5)).toEqual(['full', 'full', 'full', 'half', 'empty']);
    expect(classesOf(0)).toEqual(['empty', 'empty', 'empty', 'empty', 'empty']);
    expect(classesOf(5)).toEqual(['full', 'full', 'full', 'full', 'full']);
  });

  it('rounds to half stars and clamps to 0–5', () => {
    expect(classesOf(4.3)).toEqual(['full', 'full', 'full', 'full', 'half']);
    expect(classesOf(4.7)).toEqual(['full', 'full', 'full', 'full', 'half']);
    expect(classesOf(4.8)).toEqual(['full', 'full', 'full', 'full', 'full']);
    expect(classesOf(7)).toEqual(['full', 'full', 'full', 'full', 'full']);
    expect(classesOf(-1)).toEqual(['empty', 'empty', 'empty', 'empty', 'empty']);
  });

  it('exposes an aria-label and hides the glyphs from assistive tech', () => {
    const wrapper = mount(RatingStars, { props: { value: 4 } });
    const root = wrapper.find('.rating-stars');
    expect(root.attributes('role')).toBe('img');
    expect(root.attributes('aria-label')).toBe('4,0 von 5 Sternen');
    expect(
      wrapper.findAll('.star').every((star) => star.attributes('aria-hidden') === 'true'),
    ).toBe(true);
    expect(mount(RatingStars, { props: { value: 3.5 } }).attributes('aria-label')).toBe(
      '3,5 von 5 Sternen',
    );
    expect(
      mount(RatingStars, { props: { value: 3.5, label: 'Durchschnitt' } }).attributes('aria-label'),
    ).toBe('Durchschnitt');
  });

  it('applies the size modifier', () => {
    expect(mount(RatingStars, { props: { value: 1 } }).classes()).toContain('size-md');
    expect(mount(RatingStars, { props: { value: 1, size: 'lg' } }).classes()).toContain('size-lg');
  });
});
