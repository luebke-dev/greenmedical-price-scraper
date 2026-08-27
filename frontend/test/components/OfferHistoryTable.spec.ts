import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getOfferHistory, type OfferHistoryParams } from '@/api/endpoints';
import OfferHistoryTable from '@/components/OfferHistoryTable.vue';
import { makeOfferHistoryPage, makeOfferRow, makePhaseRow } from '../fixtures';
import { installTestPlugins } from '../helpers';

vi.mock('@/api/endpoints', () => ({ getOfferHistory: vi.fn() }));

installTestPlugins();

const fetchMock = vi.mocked(getOfferHistory);
const NOW = new Date('2026-08-27T20:00:42Z');
const RANGE_30D = {
  from: '2026-07-28T20:00:00.000Z',
  to: '2026-08-27T20:00:00.000Z',
  bucket: 'run',
} as const;

function lastParams(): OfferHistoryParams | undefined {
  return fetchMock.mock.calls[fetchMock.mock.calls.length - 1]?.[1];
}

function phases() {
  return [
    makePhaseRow({ pharmacy: 'Alpha Apotheke', from: '2026-08-27T08:00:00Z', to: null, runs: 2 }),
    makePhaseRow({
      pharmacy_id: 2,
      pharmacy: 'Zeta Apotheke',
      city: 'Berlin',
      from: '2026-08-26T08:00:00Z',
      to: '2026-08-26T14:00:00Z',
      runs: 1,
      price: 5.99,
      price_per_thc_gram: 24.96,
      availability: 'NEU',
    }),
    makePhaseRow({
      pharmacy_id: 3,
      pharmacy: 'Omega',
      from: '2026-08-25T08:00:00Z',
      to: '2026-08-25T08:00:00Z',
      runs: 1,
      price: null,
      price_per_thc_gram: null,
      availability: '',
      delisted: true,
    }),
  ];
}

async function mountTable(props: Record<string, unknown> = {}) {
  const wrapper = mount(OfferHistoryTable, {
    props: { strainId: 7, preset: '30d', now: NOW, ...props },
  });
  await flushPromises();
  return wrapper;
}

describe('OfferHistoryTable', () => {
  beforeEach(() => fetchMock.mockReset());

  it('loads the phases for the preset range and renders the "changes" columns', async () => {
    fetchMock.mockResolvedValue(makeOfferHistoryPage(phases(), { total: 3 }));
    const wrapper = await mountTable();
    expect(fetchMock).toHaveBeenCalledWith(
      7,
      { ...RANGE_30D, mode: 'changes', limit: 50, offset: 0 },
      expect.any(AbortSignal),
    );
    expect(wrapper.findAll('thead th').map((th) => th.text())).toEqual([
      'Von',
      'Bis',
      'Apotheke',
      'Stadt',
      '€/g',
      '€/g THC',
      'Status',
    ]);
    const rows = wrapper.findAll('tbody tr');
    expect(rows).toHaveLength(3);
    const first = rows[0]!.findAll('td');
    expect(first[0]!.text()).toBe('27.08.2026, 10:00');
    expect(first[1]!.find('.current').text()).toBe('aktuell');
    expect(first[1]!.find('.runs').text()).toBe('(2 Läufe)');
    expect(first[2]!.text()).toBe('Alpha Apotheke');
    expect(first[4]!.text()).toBe('6,49 €/g');
    expect(first[5]!.text()).toBe('27,04 €/g THC');
    expect(first[6]!.find('.status').text()).toBe('Auf Lager');
    const second = rows[1]!.findAll('td');
    expect(second[1]!.text()).toContain('26.08.2026, 16:00');
    expect(second[1]!.find('.runs').text()).toBe('(1 Lauf)');
    const third = rows[2]!;
    expect(third.classes()).toContain('delisted');
    expect(third.findAll('td')[4]!.text()).toBe('');
    expect(third.find('.status.delisted').text()).toBe('nicht mehr gelistet');
    expect(wrapper.find('.table-pager-top .pager-range').text()).toBe('1–3 von 3 Einträgen');
  });

  it('switches to mode=all with its own columns, day formatting and run separators', async () => {
    fetchMock.mockResolvedValue(makeOfferHistoryPage(phases(), { total: 3 }));
    const wrapper = await mountTable();
    fetchMock.mockResolvedValue(
      makeOfferHistoryPage(
        [
          makeOfferRow({ at: '2026-08-27', pharmacy: 'Alpha' }),
          makeOfferRow({ at: '2026-08-27', pharmacy: 'Beta', pharmacy_id: 2 }),
          makeOfferRow({ at: '2026-08-26', pharmacy: 'Alpha', availability: 'NEU' }),
        ],
        { mode: 'all', bucket: 'day', total: 3 },
      ),
    );
    await wrapper.find('[data-testid="offer-history-mode"]').trigger('click');
    await flushPromises();
    expect(lastParams()).toEqual({ ...RANGE_30D, mode: 'all', limit: 50, offset: 0 });
    expect(wrapper.find('[data-testid="offer-history-mode"]').text()).toBe('Alle Läufe anzeigen');
    expect(wrapper.findAll('thead th').map((th) => th.text())).toEqual([
      'Datum',
      'Apotheke',
      'Stadt',
      '€/g',
      '€/g THC',
      'Status',
    ]);
    const rows = wrapper.findAll('tbody tr');
    expect(rows.map((row) => row.classes().includes('run-start'))).toEqual([true, false, true]);
    expect(rows[0]!.findAll('td')[0]!.text()).toBe('27.08.2026');
    expect(rows[2]!.find('.status').text()).toBe('NEU');
  });

  it('pages through the server (top pager and q-table bottom) and resets on preset change', async () => {
    fetchMock.mockResolvedValue(makeOfferHistoryPage(phases(), { total: 130 }));
    const wrapper = await mountTable();
    expect(wrapper.find('.table-pager-top .pager-range').text()).toBe('1–50 von 130 Einträgen');
    expect(wrapper.find('.q-table__bottom').text()).toContain('1–50 von 130');

    const buttons = wrapper.findAll('.table-pager-top .q-pagination button');
    await buttons[buttons.length - 2]!.trigger('click');
    await flushPromises();
    expect(lastParams()).toEqual({ ...RANGE_30D, mode: 'changes', limit: 50, offset: 50 });

    await wrapper.find('.table-pager-top select').setValue('25');
    await flushPromises();
    // First visible row (50) stays in view: page 3 of 25.
    expect(lastParams()).toEqual({ ...RANGE_30D, mode: 'changes', limit: 25, offset: 50 });
    expect(wrapper.find('.table-pager-top .pager-range').text()).toBe('51–75 von 130 Einträgen');

    await wrapper.setProps({ preset: '7d' });
    await flushPromises();
    expect(lastParams()).toEqual({
      from: '2026-08-20T20:00:00.000Z',
      to: '2026-08-27T20:00:00.000Z',
      bucket: 'run',
      mode: 'changes',
      limit: 25,
      offset: 0,
    });
  });

  it('shows the loading, empty and error states and aborts superseded requests', async () => {
    let resolveFirst!: (value: ReturnType<typeof makeOfferHistoryPage>) => void;
    fetchMock.mockImplementationOnce(
      (_id, _params, signal) =>
        new Promise((resolve, reject) => {
          resolveFirst = resolve;
          signal?.addEventListener('abort', () => reject(new DOMException('x', 'AbortError')));
        }),
    );
    const wrapper = mount(OfferHistoryTable, { props: { strainId: 7, preset: '30d', now: NOW } });
    expect(wrapper.find('.offer-history').attributes('aria-busy')).toBe('true');
    expect(wrapper.find('.empty').text()).toBe('Verlauf wird geladen.');

    fetchMock.mockResolvedValueOnce(makeOfferHistoryPage([], { total: 0 }));
    await wrapper.setProps({ strainId: 8 });
    await flushPromises();
    expect(fetchMock.mock.calls[0]?.[2]?.aborted).toBe(true);
    expect(wrapper.find('.empty').text()).toBe(
      'Keine historischen Angebote im gewählten Zeitraum.',
    );
    resolveFirst(makeOfferHistoryPage(phases()));
    await flushPromises();
    expect(wrapper.findAll('tbody tr')).toHaveLength(0);

    fetchMock.mockRejectedValueOnce(new Error('boom'));
    await wrapper.setProps({ strainId: 9 });
    await flushPromises();
    expect(wrapper.find('.empty .empty-message').text()).toBe(
      'Angebotshistorie konnte nicht geladen werden.',
    );
    fetchMock.mockResolvedValueOnce(makeOfferHistoryPage(phases(), { total: 3 }));
    await wrapper.find('.empty button.empty-retry').trigger('click');
    await flushPromises();
    expect(wrapper.findAll('tbody tr')).toHaveLength(3);
  });
});
