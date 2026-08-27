<template>
  <div class="history-controls">
    <q-btn-toggle
      :model-value="preset"
      class="preset-toggle"
      :options="presetOptions"
      no-caps
      unelevated
      dense
      toggle-color="primary"
      role="group"
      :aria-label="de.history.presetsAria"
      @update:model-value="(value: HistoryPreset) => emit('update:preset', value)"
    />
    <q-toggle
      :model-value="thcMode"
      dense
      color="primary"
      :label="de.history.thcMode"
      @update:model-value="(value: boolean) => emit('update:thcMode', value)"
    />
    <q-toggle
      :model-value="pharmacies"
      dense
      color="primary"
      :label="de.history.pharmacies"
      @update:model-value="(value: boolean) => emit('update:pharmacies', value)"
    />
  </div>
</template>

<script setup lang="ts">
import { de } from '@/i18n/de';
import { HISTORY_PRESETS, type HistoryPreset } from '@/lib/history';

defineProps<{
  preset: HistoryPreset;
  thcMode: boolean;
  pharmacies: boolean;
}>();

const emit = defineEmits<{
  'update:preset': [value: HistoryPreset];
  'update:thcMode': [value: boolean];
  'update:pharmacies': [value: boolean];
}>();

const presetOptions = HISTORY_PRESETS.map((preset) => ({
  label: de.history.presets[preset],
  value: preset,
}));
</script>
