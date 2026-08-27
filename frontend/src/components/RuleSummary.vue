<template>
  <ul class="rule-summary" :aria-label="de.rules.editor.heading">
    <li v-for="rule in rules" :key="rule.id" class="rule-summary-item">
      <span class="rule-summary-kind">{{ de.rules.kinds[rule.kind].label }}</span>
      <span class="rule-summary-text">{{ ruleSummary(rule) }}</span>
    </li>
  </ul>
</template>

<script setup lang="ts">
import type { Rule } from '@/api/types';
import { de } from '@/i18n/de';
import { ruleSummary } from '@/lib/rules';

defineProps<{ rules: readonly Rule[] }>();
</script>
