<template>
  <div class="empty" :class="{ 'empty--error': tone === 'error' }" :role="role">
    <div class="empty-message">{{ message }}</div>
    <button v-if="retryLabel" type="button" class="clear-button empty-retry" @click="emit('retry')">
      {{ retryLabel }}
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';

export type EmptyStateTone = 'status' | 'error';

const props = withDefaults(
  defineProps<{ message: string; tone?: EmptyStateTone; retryLabel?: string | null }>(),
  { tone: 'status', retryLabel: null },
);

const emit = defineEmits<{ retry: [] }>();

// Errors are announced immediately; everything else is a polite status.
const role = computed(() => (props.tone === 'error' ? 'alert' : 'status'));
</script>
