<template>
  <div class="metric" :class="{ linked: hasLink }" :data-testid="`metric-${id}`">
    <div class="metric-label">{{ label }}</div>
    <div class="metric-value">
      <q-skeleton v-if="loading" type="text" width="60%" class="metric-skeleton" />
      <template v-else>{{ value }}</template>
    </div>
    <div v-if="!loading && entry" class="metric-meta">
      <div v-if="entry.name" class="meta-name">{{ entry.name }}</div>
      <div v-if="facts">{{ facts }}</div>
      <div v-if="entry.pharmacy">{{ entry.pharmacy }}</div>
    </div>
    <a
      v-if="hasLink && entry"
      class="card-link"
      :href="withoutFragment(entry.product_url)"
      target="_blank"
      rel="noopener"
      :aria-label="de.metrics.openAt(entry.name)"
      :title="de.offers.buyHint"
      @click.prevent="openProduct(entry.product_url)"
    ></a>
    <router-link
      v-if="!loading && entry && entry.strain_id"
      class="history-link"
      :to="{ name: 'strain', params: { id: entry.strain_id } }"
      :aria-label="de.metrics.historyAria(entry.name)"
    >
      {{ de.metrics.history }}
    </router-link>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { Highlight } from '@/api/types';
import { de } from '@/i18n/de';
import { openProduct, withoutFragment } from '@/lib/buy';

const props = defineProps<{
  id: string;
  label: string;
  value: string;
  entry?: Highlight | null | undefined;
  loading?: boolean | undefined;
}>();

const hasLink = computed(() => !props.loading && Boolean(props.entry?.product_url));

const facts = computed(() => {
  const entry = props.entry;
  if (!entry) return '';
  return [entry.genetics, entry.thc && `THC ${entry.thc}`, entry.cbd && `CBD ${entry.cbd}`]
    .filter(Boolean)
    .join(' · ');
});
</script>
