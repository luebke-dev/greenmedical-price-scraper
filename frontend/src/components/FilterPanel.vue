<template>
  <section v-show="open" :id="id" class="filters" :aria-label="de.filters.heading">
    <GeneticsChips
      v-if="genetics.length >= 2"
      :id="`${id}-genetics`"
      :options="genetics"
      :selected="selectedGenetics"
      @toggle="(key) => emit('toggleGenetics', key)"
    />
    <template v-for="config in RANGE_CONFIGS" :key="config.key">
      <RangeFilter
        v-if="bounds[config.key] && ranges[config.key]"
        :config="config"
        :bounds="bounds[config.key]!"
        :model-value="ranges[config.key]!"
        @update:model-value="(value) => emit('updateRange', config.key, value)"
      />
    </template>
  </section>
</template>

<script setup lang="ts">
import { de } from '@/i18n/de';
import {
  RANGE_CONFIGS,
  type BoundsState,
  type GeneticsOption,
  type RangeKey,
  type RangeState,
  type RangeValue,
} from '@/lib/filter';
import GeneticsChips from './GeneticsChips.vue';
import RangeFilter from './RangeFilter.vue';

defineProps<{
  id: string;
  open: boolean;
  genetics: readonly GeneticsOption[];
  selectedGenetics: readonly string[];
  bounds: BoundsState;
  ranges: RangeState;
}>();

const emit = defineEmits<{
  toggleGenetics: [key: string];
  updateRange: [key: RangeKey, value: RangeValue];
}>();
</script>
