<template>
  <section class="toolbar" :aria-label="de.toolbar.heading">
    <q-input
      ref="searchRef"
      class="search"
      :model-value="modelValue"
      type="search"
      borderless
      dense
      hide-bottom-space
      autocomplete="off"
      :placeholder="de.toolbar.searchPlaceholder"
      :aria-label="de.toolbar.searchLabel"
      @update:model-value="onInput"
    />
    <div class="links">
      <button
        type="button"
        class="clear-button toggle-filters"
        :class="{ open }"
        :aria-expanded="open ? 'true' : 'false'"
        :aria-controls="panelId"
        @click="emit('toggle')"
      >
        <span>{{ de.toolbar.filter }}</span>
        <span class="chevron" aria-hidden="true">▸</span>
      </button>
      <button type="button" class="clear-button" @click="onReset">
        {{ de.toolbar.reset }}
      </button>
      <div class="result-count" aria-live="polite">{{ de.toolbar.count(integer(count)) }}</div>
    </div>
  </section>
</template>

<script setup lang="ts">
import type { QInput } from 'quasar';
import { ref } from 'vue';
import { de } from '@/i18n/de';
import { integer } from '@/lib/format';

defineProps<{
  modelValue: string;
  count: number;
  open: boolean;
  panelId: string;
}>();

const emit = defineEmits<{
  'update:modelValue': [value: string];
  toggle: [];
  reset: [];
}>();

const searchRef = ref<InstanceType<typeof QInput> | null>(null);

function onInput(value: string | number | null): void {
  emit('update:modelValue', value === null ? '' : String(value));
}

function onReset(): void {
  emit('reset');
  searchRef.value?.focus();
}
</script>
