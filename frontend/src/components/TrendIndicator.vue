<template>
  <span
    v-if="trend"
    class="trend"
    :class="trendClass(trend.direction)"
    role="img"
    tabindex="0"
    :aria-label="ariaLabel"
    :data-direction="trend.direction"
  >
    <span aria-hidden="true">{{ glyph }}</span>
    <q-tooltip class="trend-tooltip" anchor="top middle" self="bottom middle" :offset="[0, 6]">
      {{ tooltip }}
    </q-tooltip>
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { Trend } from '@/api/types';
import { TREND_GLYPH, trendAriaLabel, trendClass, trendTooltip } from '@/lib/trend';

const props = defineProps<{
  trend: Trend | null;
  /** Timestamp of the latest run, used for "vor N Tagen". */
  latestAt?: string | null | undefined;
}>();

const glyph = computed(() => (props.trend ? TREND_GLYPH[props.trend.direction] : ''));
const tooltip = computed(() => (props.trend ? trendTooltip(props.trend, props.latestAt) : ''));
const ariaLabel = computed(() => (props.trend ? trendAriaLabel(props.trend, props.latestAt) : ''));
</script>
