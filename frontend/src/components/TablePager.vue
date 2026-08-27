<template>
  <div class="table-pager" :class="`table-pager-${position}`">
    <span class="result-count pager-range">{{ rangeText }}</span>
    <q-pagination
      v-if="pages > 1"
      class="pager-pages"
      :model-value="page"
      :max="pages"
      :max-pages="7"
      boundary-numbers
      boundary-links
      direction-links
      dense
      flat
      color="primary"
      :aria-label="de.pager.pagesAria"
      @update:model-value="(value) => emit('update:page', value)"
    />
    <label class="pager-size">
      <span>{{ de.pager.perPage }}</span>
      <select
        class="pager-size-select"
        :value="size"
        :aria-label="de.pager.perPageAria"
        @change="onSize"
      >
        <option v-for="option in sizes" :key="option" :value="option">{{ option }}</option>
      </select>
    </label>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { de } from '@/i18n/de';
import { integer } from '@/lib/format';
import { PAGE_SIZES } from '@/lib/url-state';

const props = withDefaults(
  defineProps<{
    page: number;
    size: number;
    total: number;
    sizes?: readonly number[];
    position?: 'top' | 'bottom';
    /** Noun for the range text, e.g. "Sorten". */
    noun?: string;
  }>(),
  { sizes: () => PAGE_SIZES, position: 'top', noun: '' },
);

const emit = defineEmits<{ 'update:page': [page: number]; 'update:size': [size: number] }>();

const pages = computed(() => Math.max(1, Math.ceil(props.total / Math.max(1, props.size))));

const rangeText = computed(() => {
  if (props.total === 0) return de.pager.none(props.noun);
  const from = (props.page - 1) * props.size + 1;
  const to = Math.min(props.total, props.page * props.size);
  return de.pager.range(integer(from), integer(to), integer(props.total), props.noun);
});

function onSize(event: Event): void {
  const value = Number((event.target as HTMLSelectElement).value);
  if (Number.isFinite(value)) emit('update:size', value);
}
</script>
