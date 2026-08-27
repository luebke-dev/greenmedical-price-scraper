import { flushPromises, mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import StrainTable from '@/components/StrainTable.vue';
import { LATEST_AT, makeListItem, makeOffer, makeTrend } from '../fixtures';
import { installTestPlugins } from '../helpers';

installTestPlugins();

function rows() {
  return [
    makeListItem({
      id: 7,
      name: 'OG Kush',
      bezeichnung: 'Cannamedical CM 24/1',
      genetik: 'Hybrid Sativa Dominant',
      thc: '24%',
      cbd: '1%',
      price: 6.49,
      thcPrice: 27.04,
      trend: makeTrend(),
      ratingValue: 4.3,
      reviewCount: 124,
      offers: [
        makeOffer({
          apotheke: 'Grüne Blüte',
          apotheke_stadt: 'Markkleeberg',
          preis_pro_gramm: '6,49 €/g',
        }),
        makeOffer({
          apotheke: 'Apo Zwei',
          apotheke_stadt: 'Leipzig',
          preis_pro_gramm: '7,49 €/g',
          verfuegbarkeit: 'NEU',
        }),
      ],
    }),
    makeListItem({ id: 8, name: 'Bunatic', price: null, thcPrice: null }),
  ];
}

function mountTable(overrides: Record<string, unknown> = {}) {
  return mount(StrainTable, {
    props: {
      rows: rows(),
      sort: { key: 'price', direction: 'asc' },
      page: 1,
      size: 50,
      total: 2,
      latestAt: LATEST_AT,
      ...overrides,
    },
  });
}

describe('StrainTable', () => {
  it('renders the 9 column headers with aria-sort and sort buttons', () => {
    const wrapper = mountTable();
    const headers = wrapper.findAll('thead th');
    expect(headers).toHaveLength(9);
    expect(headers.map((th) => th.find('button span').text())).toEqual([
      'Sorte',
      'Bezeichnung',
      'ab €/g',
      'ab €/g THC',
      'THC',
      'CBD',
      'Genetik',
      'Apotheken',
      'Bewertung',
    ]);
    expect(headers.map((th) => th.attributes('aria-sort'))).toEqual([
      'none',
      'none',
      'ascending',
      'none',
      'none',
      'none',
      'none',
      'none',
      'none',
    ]);
    expect(headers[2]!.find('.sort-indicator').text()).toBe('^');
    expect(headers[0]!.attributes('style')).toContain('width: 18%');
  });

  it('emits sort with the column key when a header button is clicked', async () => {
    const wrapper = mountTable({ sort: { key: 'thc', direction: 'desc' } });
    expect(wrapper.find('th[data-key="thc"]').attributes('aria-sort')).toBe('descending');
    expect(wrapper.find('th[data-key="thc"] .sort-indicator').text()).toBe('v');
    await wrapper.find('th[data-key="name"] button').trigger('click');
    expect(wrapper.emitted('sort')).toEqual([['name']]);
  });

  it('renders the cells like site/app.js', () => {
    const wrapper = mountTable();
    const first = wrapper.findAll('tr.group-row')[0]!;
    const cells = first.findAll('td');
    expect(cells).toHaveLength(9);
    expect(cells[0]!.text()).toContain('OG Kush');
    expect(cells[1]!.text()).toBe('Cannamedical CM 24/1');
    expect(cells[2]!.text()).toContain('ab 6,49 €/g');
    expect(cells[2]!.classes()).toContain('price');
    expect(cells[3]!.text()).toBe('ab 27,04 €/g THC');
    expect(cells[4]!.text()).toBe('24%');
    expect(cells[5]!.text()).toBe('1%');
    expect(cells[6]!.text()).toBe('Hybrid Sativa Dominant');
    expect(cells[7]!.text()).toBe('2');
    expect(cells[8]!.text()).toBe('★ 4,3 (124)');

    const second = wrapper.findAll('tr.group-row')[1]!.findAll('td');
    expect(second[2]!.text()).toBe('');
    expect(second[3]!.text()).toBe('');
    expect(second[8]!.text()).toBe('');
  });

  it('renders the rating cell compactly with an accessible label', async () => {
    const wrapper = mountTable({
      rows: [
        makeListItem({ id: 1, ratingValue: 4.3, reviewCount: 124 }),
        makeListItem({ id: 2, ratingValue: null, reviewCount: 0 }),
        makeListItem({ id: 3, ratingValue: 5, reviewCount: 1 }),
        makeListItem({ id: 4 }),
      ],
    });
    const cells = wrapper.findAll('tr.group-row td[data-key="rating"]');
    expect(cells[0]!.classes()).toContain('rating');
    expect(cells[0]!.find('.rating-compact').attributes('aria-label')).toBe(
      '4,3 von 5 Sternen, 124 Bewertungen',
    );
    expect(cells[0]!.text()).toBe('★ 4,3 (124)');
    expect(cells[1]!.find('.rating-compact').exists()).toBe(false);
    expect(cells[2]!.text()).toBe('★ 5,0 (1)');
    expect(cells[2]!.find('.rating-compact').attributes('aria-label')).toBe(
      '5,0 von 5 Sternen, 1 Bewertung',
    );
    expect(cells[3]!.text()).toBe('');
    await wrapper.find('th[data-key="rating"] button').trigger('click');
    expect(wrapper.emitted('sort')).toEqual([['rating']]);
  });

  it('shows the trend glyph inside the "ab €/g" cell', () => {
    const wrapper = mountTable();
    const trend = wrapper.find('tr.group-row td[data-key="price"] .trend');
    expect(trend.exists()).toBe(true);
    expect(trend.text()).toBe('▼');
    expect(trend.attributes('aria-label')).toContain('vor 7 Tagen: 6,49 €/g');
    expect(wrapper.findAll('tr.group-row')[1]!.find('.trend').exists()).toBe(false);
  });

  it('links every strain to its detail page and navigates on row click', async () => {
    const wrapper = mountTable();
    const row = wrapper.find('tr.group-row[data-id="7"]');
    expect(row.attributes('role')).toBe('link');
    expect(row.attributes('tabindex')).toBe('0');
    expect(row.find('a.strain-name').attributes('href')).toBe('/sorte/7');
    // No inline offers on the overview: pharmacies live on /sorte/:id.
    expect(wrapper.find('tr.detail-row').exists()).toBe(false);
    expect(wrapper.find('table.offers').exists()).toBe(false);
    await row.trigger('click');
    await flushPromises();
    expect(wrapper.vm.$router.currentRoute.value.path).toBe('/sorte/7');
  });

  it('paginates on the server: pager top + q-table bottom, rowsNumber from total', async () => {
    const wrapper = mountTable({ page: 2, size: 25, total: 120 });
    // Top pager: range text, pages, size select.
    const top = wrapper.find('.table-pager-top');
    expect(top.find('.pager-range').text()).toBe('26–50 von 120 Sorten');
    const select = top.find('select.pager-size-select');
    expect(select.findAll('option').map((option) => option.text())).toEqual(['25', '50', '100']);
    expect((select.element as HTMLSelectElement).value).toBe('25');
    expect(top.find('.q-pagination').exists()).toBe(true);
    expect(
      top.find('.q-pagination [aria-current="true"], .q-pagination .q-btn--standard').text(),
    ).toContain('2');

    // The q-table is in server mode: it shows all given rows and its own bottom controls.
    expect(wrapper.findAll('tr.group-row')).toHaveLength(2);
    expect(wrapper.find('.q-table__bottom').exists()).toBe(true);
    expect(wrapper.find('.q-table__bottom').text()).toContain('26–50 von 120');

    await select.setValue('100');
    expect(wrapper.emitted('paginate')).toEqual([[{ page: 2, size: 100 }]]);

    // Next page via the top pager.
    const buttons = top.findAll('.q-pagination button');
    const next = buttons[buttons.length - 2]!;
    await next.trigger('click');
    expect(wrapper.emitted('paginate')?.[1]).toEqual([{ page: 3, size: 25 }]);

    // …and via q-table's own controls at the bottom.
    const bottomButtons = wrapper.findAll('.q-table__bottom button');
    await bottomButtons[bottomButtons.length - 2]!.trigger('click'); // next

    expect(wrapper.emitted('paginate')?.[2]).toEqual([{ page: 3, size: 25 }]);
  });

  it('does not emit paginate when q-table reports the unchanged pagination', async () => {
    const wrapper = mountTable({ page: 1, size: 50, total: 2 });
    expect(wrapper.find('.table-pager-top .q-pagination').exists()).toBe(false);
    expect(wrapper.find('.table-pager-top .pager-range').text()).toBe('1–2 von 2 Sorten');
    const table = wrapper.findComponent({ name: 'QTable' });
    table.vm.$emit('request', { pagination: { page: 1, rowsPerPage: 50, rowsNumber: 2 } });
    await flushPromises();
    expect(wrapper.emitted('paginate')).toBeUndefined();
  });

  it('shows empty, loading and error states', async () => {
    const wrapper = mountTable({ rows: [], total: 0 });
    expect(wrapper.find('.empty').text()).toBe('Keine Sorten gefunden.');
    expect(wrapper.find('.empty-retry').exists()).toBe(false);
    await wrapper.setProps({ loading: true });
    expect(wrapper.find('.empty').text()).toBe('Daten werden geladen.');
    expect(wrapper.find('.empty-retry').exists()).toBe(false);
    await wrapper.setProps({ loading: false, error: 'Daten konnten nicht geladen werden.' });
    expect(wrapper.find('.empty .empty-message').text()).toBe(
      'Daten konnten nicht geladen werden.',
    );
    expect(wrapper.find('.empty').attributes('role')).toBe('alert');
  });

  it('offers "Erneut laden" in the error state and emits retry', async () => {
    const wrapper = mountTable({ rows: [], total: 0, error: 'Noch keine Daten vorhanden.' });
    const retry = wrapper.find('.empty button.empty-retry');
    expect(retry.text()).toBe('Erneut laden');
    await retry.trigger('click');
    expect(wrapper.emitted('retry')).toEqual([[]]);
  });
});
