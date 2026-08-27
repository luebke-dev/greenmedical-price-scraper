import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ReviewsSection from '@/components/ReviewsSection.vue';
import { useReviewsStore, type ReviewsEntry, type ReviewsQuery } from '@/stores/reviews';
import { LATEST_AT, makeReview, makeReviewsResponse } from '../fixtures';
import { installTestPlugins } from '../helpers';

installTestPlugins();

type Fetch = (id: number, query: ReviewsQuery) => Promise<ReviewsEntry>;

const FIRST: ReviewsQuery = { sort: 'newest', limit: 25, offset: 0 };

/** Stubs the store actions; the component only talks to fetchPage/abortAll. */
function stubStore(fetchPage: Fetch) {
  const store = useReviewsStore();
  const fetchSpy = vi.spyOn(store, 'fetchPage').mockImplementation(fetchPage);
  const abortSpy = vi.spyOn(store, 'abortAll').mockImplementation(() => {});
  return { store, fetchSpy, abortSpy };
}

function entryOf(reviews = 3, overrides: Partial<ReviewsEntry['summary']> = {}): ReviewsEntry {
  const list = Array.from({ length: reviews }, (_, index) =>
    makeReview({
      id: index + 1,
      author: `Autor ${index + 1}`,
      rating: 5 - index,
      verified: index % 2 === 0,
      reviewed_on: `2026-08-2${index + 1}`,
      content: `Text ${index + 1}`,
    }),
  );
  const response = makeReviewsResponse(list, undefined, {
    value: 4.3,
    count: 124,
    verified_count: 93,
    stored_count: 124,
    distribution: { '1': 4, '2': 6, '3': 14, '4': 40, '5': 60 },
    ...overrides,
  });
  return {
    summary: response.summary,
    reviews: response.reviews,
    total: 124,
    limit: 25,
    offset: 0,
  };
}

async function mountSection(id = 7) {
  const wrapper = mount(ReviewsSection, { props: { strainId: id } });
  await flushPromises();
  return wrapper;
}

describe('ReviewsSection', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('shows the loading state, then summary, distribution and list', async () => {
    let resolve!: (entry: ReviewsEntry) => void;
    const { fetchSpy } = stubStore(() => new Promise((res) => (resolve = res)));
    const wrapper = mount(ReviewsSection, { props: { strainId: 7 } });
    expect(wrapper.find('.reviews').attributes('aria-busy')).toBe('true');
    expect(wrapper.find('.empty').text()).toBe('Bewertungen werden geladen.');
    expect(fetchSpy).toHaveBeenCalledWith(7, FIRST);

    resolve(entryOf());
    await flushPromises();
    expect(wrapper.find('.reviews').attributes('aria-busy')).toBe('false');
    expect(wrapper.find('h3').text()).toBe('Bewertungen');
    expect(wrapper.find('[data-testid="reviews-average"]').text()).toBe('4,3');
    expect(wrapper.find('.reviews-average .rating-stars').attributes('aria-label')).toBe(
      '4,5 von 5 Sternen',
    );
    expect(wrapper.find('[data-testid="reviews-count"]').text()).toBe('124 Bewertungen');
    expect(wrapper.find('[data-testid="reviews-verified"]').text()).toBe('75 % verifizierte Käufe');
    expect(wrapper.find('.reviews-asof').text()).toContain('Stand:');
    expect(wrapper.find('.reviews-asof').text()).toContain('2026');

    const rows = wrapper.findAll('.reviews-distribution li');
    expect(rows.map((row) => row.attributes('aria-label'))).toEqual([
      '5 Sterne: 60',
      '4 Sterne: 40',
      '3 Sterne: 14',
      '2 Sterne: 6',
      '1 Stern: 4',
    ]);
    expect(rows.map((row) => row.find('.distribution-fill').attributes('style'))).toEqual([
      'width: 100%;',
      'width: 67%;',
      'width: 23%;',
      'width: 10%;',
      'width: 7%;',
    ]);

    const list = wrapper.find('ol.reviews-list');
    expect(list.attributes('aria-label')).toBe('Liste der Bewertungen');
    const items = list.findAll('li.review');
    expect(items).toHaveLength(3);
    expect(items[0]!.find('.review-author').text()).toBe('Autor 1');
    expect(items[0]!.find('.review-date').text()).toBe('21.08.2026');
    expect(items[0]!.find('.rating-stars').attributes('aria-label')).toBe('5,0 von 5 Sternen');
    expect(items[0]!.find('.review-verified').text()).toBe('Verifizierter Kauf');
    expect(items[0]!.find('.review-text').text()).toBe('Text 1');
    expect(items[1]!.find('.review-verified').exists()).toBe(false);
    expect(items[1]!.find('.rating-stars').attributes('aria-label')).toBe('4,0 von 5 Sternen');
  });

  it('pages with limit/offset (top + bottom controls) and re-fetches on sort change', async () => {
    const { fetchSpy } = stubStore((_id, query) =>
      Promise.resolve({ ...entryOf(Math.min(query.limit, 124 - query.offset)), ...query }),
    );
    const wrapper = await mountSection();
    const pagers = wrapper.findAll('.table-pager');
    expect(pagers).toHaveLength(2);
    expect(pagers[0]!.find('.pager-range').text()).toBe('1–25 von 124 Bewertungen');
    expect(pagers[1]!.classes()).toContain('table-pager-bottom');
    expect(wrapper.findAll('li.review')).toHaveLength(25);

    const buttons = pagers[1]!.findAll('.q-pagination button');
    await buttons[buttons.length - 2]!.trigger('click'); // next
    await flushPromises();
    expect(fetchSpy).toHaveBeenLastCalledWith(7, { sort: 'newest', limit: 25, offset: 25 });
    expect(wrapper.find('.table-pager-top .pager-range').text()).toBe('26–50 von 124 Bewertungen');

    await wrapper.find('.table-pager-top select').setValue('100');
    await flushPromises();
    // Row 25 stays in view → page 1 of 100.
    expect(fetchSpy).toHaveBeenLastCalledWith(7, { sort: 'newest', limit: 100, offset: 0 });
    expect(wrapper.findAll('li.review')).toHaveLength(100);

    const select = wrapper.find('[data-testid="reviews-sort"]');
    expect(select.findAll('option').map((option) => option.text())).toEqual([
      'Neueste zuerst',
      'Älteste zuerst',
      'Beste zuerst',
      'Schlechteste zuerst',
    ]);
    await wrapper.find('.table-pager-top .q-pagination button:last-child').trigger('click'); // last
    await flushPromises();
    expect(fetchSpy).toHaveBeenLastCalledWith(7, { sort: 'newest', limit: 100, offset: 100 });
    await select.setValue('lowest');
    await flushPromises();
    // Sort change → back to page 1.
    expect(fetchSpy).toHaveBeenLastCalledWith(7, { sort: 'lowest', limit: 100, offset: 0 });
  });

  it('hides the bottom pager when everything fits on one page', async () => {
    const entry = entryOf(3);
    stubStore(() => Promise.resolve({ ...entry, total: 3 }));
    const wrapper = await mountSection();
    expect(wrapper.findAll('.table-pager')).toHaveLength(1);
    expect(wrapper.find('.table-pager .q-pagination').exists()).toBe(false);
  });

  it('shows "Noch keine Bewertungen" for a scraped strain without reviews', async () => {
    stubStore(() =>
      Promise.resolve({
        summary: {
          value: null,
          count: 0,
          scraped_at: LATEST_AT,
          distribution: { '1': 0, '2': 0, '3': 0, '4': 0, '5': 0 },
          verified_count: 0,
          stored_count: 0,
        },
        reviews: [],
        total: 0,
        limit: 25,
        offset: 0,
      }),
    );
    const wrapper = await mountSection();
    expect(wrapper.find('.empty').text()).toBe('Noch keine Bewertungen');
    expect(wrapper.find('[data-testid="reviews-sort"]').exists()).toBe(false);
    expect(wrapper.find('.reviews-summary').exists()).toBe(false);
  });

  it('shows "Bewertungen noch nicht erfasst" when scraped_at is null', async () => {
    stubStore(() =>
      Promise.resolve({
        summary: {
          value: null,
          count: 0,
          scraped_at: null,
          distribution: { '1': 0, '2': 0, '3': 0, '4': 0, '5': 0 },
          verified_count: 0,
          stored_count: 0,
        },
        reviews: [],
        total: 0,
        limit: 25,
        offset: 0,
      }),
    );
    const wrapper = await mountSection();
    expect(wrapper.find('.empty').text()).toBe('Bewertungen noch nicht erfasst');
  });

  it('shows the error state with retry', async () => {
    const { fetchSpy } = stubStore(() => Promise.reject(new Error('boom')));
    const wrapper = await mountSection();
    expect(wrapper.find('.empty').attributes('role')).toBe('alert');
    expect(wrapper.find('.empty .empty-message').text()).toBe(
      'Bewertungen konnten nicht geladen werden.',
    );
    fetchSpy.mockImplementation(() => Promise.resolve(entryOf(1)));
    await wrapper.find('.empty button.empty-retry').trigger('click');
    await flushPromises();
    expect(wrapper.findAll('li.review')).toHaveLength(1);
  });

  it('reloads when the strain changes and aborts on unmount', async () => {
    const { fetchSpy, abortSpy } = stubStore(() => Promise.resolve(entryOf(1)));
    const wrapper = await mountSection(7);
    await wrapper.setProps({ strainId: 8 });
    await flushPromises();
    expect(fetchSpy).toHaveBeenLastCalledWith(8, FIRST);
    wrapper.unmount();
    expect(abortSpy).toHaveBeenCalled();
  });
});
