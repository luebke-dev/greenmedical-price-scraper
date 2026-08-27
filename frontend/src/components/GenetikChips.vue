<template>
  <div class="filter filter-genetik">
    <div class="filter-head">
      <span class="filter-label" :id="`${id}-label`">{{ de.filters.genetik }}</span>
    </div>
    <div class="chips" role="group" :aria-labelledby="`${id}-label`">
      <q-chip
        v-for="option in options"
        :key="option.key"
        class="chip"
        :class="{ active: isSelected(option.key) }"
        clickable
        :selected="isSelected(option.key)"
        :data-key="option.key"
        @click="emit('toggle', option.key)"
      >
        {{ option.label }}
      </q-chip>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { de } from '@/i18n/de';
import type { GenetikOption } from '@/lib/filter';

const props = defineProps<{
  id: string;
  options: readonly GenetikOption[];
  selected: readonly string[];
}>();

const emit = defineEmits<{ toggle: [key: string] }>();

const selectedSet = computed(() => new Set(props.selected));

function isSelected(key: string): boolean {
  return selectedSet.value.has(key);
}
</script>
