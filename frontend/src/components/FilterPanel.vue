<template>
  <section v-show="open" :id="id" class="filters" :aria-label="de.filters.heading">
    <GenetikChips
      v-if="genetik.length >= 2"
      :id="`${id}-genetik`"
      :options="genetik"
      :selected="selectedGenetik"
      @toggle="(key) => emit('toggleGenetik', key)"
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
  type GenetikOption,
  type RangeKey,
  type RangeState,
  type RangeValue,
} from '@/lib/filter';
import GenetikChips from './GenetikChips.vue';
import RangeFilter from './RangeFilter.vue';

defineProps<{
  id: string;
  open: boolean;
  genetik: readonly GenetikOption[];
  selectedGenetik: readonly string[];
  bounds: BoundsState;
  ranges: RangeState;
}>();

const emit = defineEmits<{
  toggleGenetik: [key: string];
  updateRange: [key: RangeKey, value: RangeValue];
}>();
</script>
