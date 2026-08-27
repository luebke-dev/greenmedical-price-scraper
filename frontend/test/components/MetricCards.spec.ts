import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import MetricCards from '@/components/MetricCards.vue';
import { makeMetadata } from '../fixtures';
import { installTestPlugins } from '../helpers';

installTestPlugins();

const LABELS = [
  'Angebote',
  'Apotheken',
  'Sorten',
  'Günstigster €/g',
  'Günstigster €/g THC',
  'Günstigster €/g CBD',
  'Höchster THC',
  'Höchster CBD',
  'Höchster THC & CBD',
  'Bestbewertet',
];

describe('MetricCards', () => {
  it('renders 10 skeleton cards (labels only) while loading', () => {
    const wrapper = mount(MetricCards, { props: { metadata: null, loading: true } });
    const cards = wrapper.findAll('.metric');
    expect(cards).toHaveLength(10);
    expect(cards.map((card) => card.find('.metric-label').text())).toEqual(LABELS);
    expect(wrapper.findAll('.q-skeleton')).toHaveLength(10);
    expect(wrapper.findAll('.metric-meta')).toHaveLength(0);
    expect(wrapper.findAll('a.card-link')).toHaveLength(0);
    expect(wrapper.find('.metrics').attributes('aria-busy')).toBe('true');
  });

  it('renders values, meta lines, product link and history link when filled', () => {
    const wrapper = mount(MetricCards, { props: { metadata: makeMetadata(), loading: false } });
    const cards = wrapper.findAll('.metric');
    expect(cards).toHaveLength(10);
    expect(wrapper.findAll('.q-skeleton')).toHaveLength(0);
    expect(wrapper.find('.metrics').attributes('aria-busy')).toBe('false');

    expect(cards[0]!.find('.metric-value').text()).toBe('2.021');
    expect(cards[1]!.find('.metric-value').text()).toBe('18');
    expect(cards[2]!.find('.metric-value').text()).toBe('912');
    expect(cards[3]!.find('.metric-value').text()).toBe('5,49 €/g');
    expect(cards[4]!.find('.metric-value').text()).toBe('20,33 €/g THC');
    expect(cards[5]!.find('.metric-value').text()).toBe('549,00 €/g CBD');
    expect(cards[6]!.find('.metric-value').text()).toBe('31%');
    expect(cards[7]!.find('.metric-value').text()).toBe('12%');
    expect(cards[8]!.find('.metric-value').text()).toBe('20% · 10%');
    expect(cards[9]!.find('.metric-value').text()).toBe('★ 4,7 (13)');

    // Plain count cards have no meta / links.
    expect(cards[0]!.find('.metric-meta').exists()).toBe(false);
    expect(cards[0]!.find('a').exists()).toBe(false);

    const cheapest = cards[3]!;
    expect(cheapest.classes()).toContain('linked');
    expect(cheapest.find('.meta-name').text()).toBe('Bunatic');
    expect(cheapest.findAll('.metric-meta div').map((d) => d.text())).toEqual([
      'Bunatic',
      'Indica · THC 27% · CBD 1%',
      'Grüne Blüte',
    ]);
    const link = cheapest.find('a.card-link');
    expect(link.attributes('href')).toBe('https://greenmedical.health/de/cannabis/flower/bunatic');
    expect(link.attributes('target')).toBe('_blank');
    expect(link.attributes('rel')).toBe('noopener');
    expect(link.attributes('aria-label')).toBe('Bunatic bei greenmedical öffnen');

    const history = cheapest.find('a.history-link');
    expect(history.text()).toBe('Verlauf →');
    expect(history.attributes('href')).toBe('/sorte/7');
    expect(history.attributes('aria-label')).toBe('Preisverlauf von Bunatic anzeigen');
    expect(cards[5]!.find('a.history-link').attributes('href')).toBe('/sorte/8');
  });

  it('leaves highlight cards empty when the highlight is null', () => {
    const wrapper = mount(MetricCards, {
      props: {
        metadata: makeMetadata({ cheapest_cbd_gram: null, highest_thc_cbd: null }),
        loading: false,
      },
    });
    const cards = wrapper.findAll('.metric');
    expect(cards[5]!.find('.metric-value').text()).toBe('');
    expect(cards[5]!.find('a').exists()).toBe(false);
    expect(cards[5]!.classes()).not.toContain('linked');
    expect(cards[8]!.find('.metric-value').text()).toBe('');
  });

  it('renders the "Bestbewertet" card with meta lines, overlay link and history link', () => {
    const wrapper = mount(MetricCards, { props: { metadata: makeMetadata(), loading: false } });
    const card = wrapper.find('[data-testid="metric-best-rated"]');
    expect(card.find('.metric-label').text()).toBe('Bestbewertet');
    expect(card.find('.metric-value').text()).toBe('★ 4,7 (13)');
    expect(card.findAll('.metric-meta div').map((d) => d.text())).toEqual([
      'Bunatic',
      'Indica · THC 27% · CBD 1%',
      'Grüne Blüte',
    ]);
    expect(card.find('a.card-link').attributes('href')).toBe(
      'https://greenmedical.health/de/cannabis/flower/bunatic',
    );
    expect(card.find('a.history-link').attributes('href')).toBe('/sorte/7');
  });

  it('leaves "Bestbewertet" empty without best_rated or without a rating value', () => {
    const empty = mount(MetricCards, {
      props: { metadata: makeMetadata({ best_rated: null }), loading: false },
    });
    expect(empty.find('[data-testid="metric-best-rated"] .metric-value').text()).toBe('');
    const base = makeMetadata().best_rated!;
    const noValue = mount(MetricCards, {
      props: {
        metadata: makeMetadata({ best_rated: { ...base, rating_value: null, review_count: 0 } }),
        loading: false,
      },
    });
    expect(noValue.find('[data-testid="metric-best-rated"] .metric-value').text()).toBe('');
  });
});
