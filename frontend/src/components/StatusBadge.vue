<template>
  <span class="status" :class="{ new: isNew }">{{ value }}</span>
</template>

<script setup lang="ts">
import { computed } from 'vue';

const props = defineProps<{ value: string }>();

// "NEU" (any casing) gets the amber highlight, exactly like site/app.js.
const isNew = computed(() => props.value.toLowerCase().includes('neu'));
</script>
