<template>
  <div class="filter filter-range" :data-key="config.key">
    <div class="filter-head">
      <span class="filter-label">{{ config.label }}</span>
      <span class="filter-value">{{ valueText }}</span>
    </div>
    <q-range
      class="range"
      :model-value="rangeModel"
      :min="bounds.min"
      :max="bounds.max"
      :step="config.step"
      snap
      dense
      color="primary"
      :left-thumb-aria-label="`${config.label} ${de.filters.minimum}`"
      :right-thumb-aria-label="`${config.label} ${de.filters.maximum}`"
      :left-label-value="format(modelValue.lo)"
      :right-label-value="format(modelValue.hi)"
      @update:model-value="onUpdate"
    />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { de } from '@/i18n/de';
import {
  clampRange,
  roundToStep,
  type RangeBounds,
  type RangeConfig,
  type RangeValue,
} from '@/lib/filter';
import { num } from '@/lib/format';

const props = defineProps<{
  config: RangeConfig;
  bounds: RangeBounds;
  modelValue: RangeValue;
}>();

const emit = defineEmits<{ 'update:modelValue': [value: RangeValue] }>();

const rangeModel = computed(() => ({ min: props.modelValue.lo, max: props.modelValue.hi }));

function format(value: number): string {
  return `${num(value, props.config.decimals)}${props.config.unit}`;
}

const valueText = computed(() => `${format(props.modelValue.lo)} – ${format(props.modelValue.hi)}`);

function onUpdate(value: { min: number | null; max: number | null }): void {
  const lo = roundToStep(value.min ?? props.bounds.min, props.config.step);
  const hi = roundToStep(value.max ?? props.bounds.max, props.config.step);
  const next = clampRange({ lo, hi }, props.bounds);
  if (next.lo === props.modelValue.lo && next.hi === props.modelValue.hi) return;
  emit('update:modelValue', next);
}
</script>
