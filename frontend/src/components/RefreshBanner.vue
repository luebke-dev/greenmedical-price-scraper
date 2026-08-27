<template>
  <div
    v-show="visible"
    class="refresh-banner"
    :class="{ 'is-running': phase === 'running', 'is-overdue': phase === 'overdue' }"
    role="status"
    aria-live="polite"
  >
    <template v-if="phase !== 'hidden'">
      <q-spinner
        v-if="phase === 'running' && !reducedMotion"
        class="refresh-spinner"
        size="0.9em"
        aria-hidden="true"
      />
      <span v-else class="refresh-dot" aria-hidden="true"></span>
      <span class="refresh-text">{{ text }}</span>
    </template>
    <span v-if="updated" class="refresh-updated">{{ de.refresh.updated }}</span>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { usePrefersReducedMotion } from '@/composables/usePrefersReducedMotion';
import type { RefreshPhase } from '@/composables/useRefreshSchedule';
import { de } from '@/i18n/de';

const props = defineProps<{ phase: RefreshPhase; text: string; updated: boolean }>();

const reducedMotion = usePrefersReducedMotion();
// Hidden when nothing is scheduled and nothing is running – unless the "updated" note is due.
const visible = computed(() => props.phase !== 'hidden' || props.updated);
</script>
