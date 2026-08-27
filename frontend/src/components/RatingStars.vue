<template>
  <span
    class="rating-stars"
    :class="`size-${size}`"
    role="img"
    :aria-label="ariaLabel"
    :data-value="value"
  >
    <span
      v-for="star in STARS"
      :key="star"
      class="star"
      :class="fillClass(star)"
      aria-hidden="true"
    ></span>
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { de } from '@/i18n/de';
import { rating } from '@/lib/format';

const STARS = [1, 2, 3, 4, 5] as const;

const props = withDefaults(
  defineProps<{
    /** 0–5, half steps supported (rounded to the nearest half). */
    value: number;
    size?: 'sm' | 'md' | 'lg';
    /** Overrides the default "x von 5 Sternen" label. */
    label?: string | undefined;
  }>(),
  { size: 'md', label: undefined },
);

const clamped = computed(() => Math.min(5, Math.max(0, Math.round(props.value * 2) / 2)));

const ariaLabel = computed(() => props.label ?? de.rating.stars(rating(clamped.value)));

function fillClass(star: number): 'full' | 'half' | 'empty' {
  if (clamped.value >= star) return 'full';
  if (clamped.value + 0.5 >= star) return 'half';
  return 'empty';
}
</script>
