<template>
  <section class="metrics" :aria-label="de.metrics.heading" :aria-busy="loading ? 'true' : 'false'">
    <MetricCard
      v-for="card in cards"
      :key="card.id"
      :id="card.id"
      :label="card.label"
      :value="card.value"
      :entry="card.entry"
      :loading="loading"
    />
  </section>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { Highlight, Metadata } from '@/api/types';
import { de } from '@/i18n/de';
import { euro, integer, ratingCompact } from '@/lib/format';
import MetricCard from './MetricCard.vue';

const props = defineProps<{ metadata: Metadata | null; loading?: boolean }>();

interface CountConfig {
  id: string;
  label: string;
  value: (metadata: Metadata) => string;
}

interface HighlightConfig {
  id: string;
  label: string;
  entry: (metadata: Metadata) => Highlight | null;
  value: (entry: Highlight) => string;
}

type MetricConfig = CountConfig | HighlightConfig;

// One entry per metric card, rendered in order (the 9 cards of site/app.js + „Bestbewertet“).
const METRIC_CONFIGS: readonly MetricConfig[] = [
  { id: 'total', label: de.metrics.offers, value: (m: Metadata) => integer(m.total) },
  {
    id: 'pharmacies',
    label: de.metrics.pharmacies,
    value: (m: Metadata) => integer(m.pharmacy_count),
  },
  { id: 'strains', label: de.metrics.strains, value: (m: Metadata) => integer(m.strain_count) },
  {
    id: 'cheapest-gram',
    label: de.metrics.cheapestGram,
    entry: (m: Metadata) => m.cheapest_gram,
    value: (entry: Highlight) => euro(entry.price, '€/g'),
  },
  {
    id: 'cheapest-thc-gram',
    label: de.metrics.cheapestThcGram,
    entry: (m: Metadata) => m.cheapest_thc_gram,
    value: (entry: Highlight) => euro(entry.price, '€/g THC'),
  },
  {
    id: 'cheapest-cbd-gram',
    label: de.metrics.cheapestCbdGram,
    entry: (m: Metadata) => m.cheapest_cbd_gram,
    value: (entry: Highlight) => euro(entry.price, '€/g CBD'),
  },
  {
    id: 'highest-thc',
    label: de.metrics.highestThc,
    entry: (m: Metadata) => m.highest_thc,
    value: (entry: Highlight) => entry.thc || '',
  },
  {
    id: 'highest-cbd',
    label: de.metrics.highestCbd,
    entry: (m: Metadata) => m.highest_cbd,
    value: (entry: Highlight) => entry.cbd || '',
  },
  {
    id: 'highest-thc-cbd',
    label: de.metrics.highestThcCbd,
    entry: (m: Metadata) => m.highest_thc_cbd,
    value: (entry: Highlight) => [entry.thc, entry.cbd].filter(Boolean).join(' · '),
  },
  {
    id: 'best-rated',
    label: de.metrics.bestRated,
    entry: (m: Metadata) => m.best_rated,
    value: (entry: Highlight) => {
      const text = ratingCompact(entry.rating_value, entry.review_count);
      return text ? `★ ${text}` : '';
    },
  },
];

interface CardModel {
  id: string;
  label: string;
  value: string;
  entry: Highlight | null;
}

const cards = computed<CardModel[]>(() =>
  METRIC_CONFIGS.map((config) => {
    const metadata = props.metadata;
    if (!metadata) return { id: config.id, label: config.label, value: '', entry: null };
    if ('entry' in config) {
      const entry = config.entry(metadata);
      return { id: config.id, label: config.label, value: entry ? config.value(entry) : '', entry };
    }
    return { id: config.id, label: config.label, value: config.value(metadata), entry: null };
  }),
);
</script>
