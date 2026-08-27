<template>
  <a
    class="buy"
    :href="href"
    target="_blank"
    rel="noopener"
    :aria-label="ariaLabel"
    :title="de.offers.buyHint"
    @click.prevent="openProduct(url)"
  >
    {{ de.offers.buy }}
  </a>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { de } from '@/i18n/de';
import { openProduct, withoutFragment } from '@/lib/buy';

const props = defineProps<{ url: string; pharmacy?: string | undefined }>();

const ariaLabel = computed(() => de.offers.buyAria(props.pharmacy ?? ''));
const href = computed(() => withoutFragment(props.url));
</script>
