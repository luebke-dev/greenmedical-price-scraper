<template>
  <div class="title-row">
    <div>
      <div class="brand">
        <span class="brand-dot" aria-hidden="true"></span>
        <h1>
          <router-link class="brand-link" :to="navigation.indexLocation">{{
            de.app.title
          }}</router-link>
        </h1>
      </div>
      <div class="updated">
        {{ de.app.updated }}
        <time v-if="updatedAt" :datetime="metadata?.generated_at">{{ updatedAt }}</time>
        <span v-else>{{ de.app.noDate }}</span>
      </div>
    </div>
    <nav class="links" :aria-label="de.app.downloads">
      <a :href="EXPORT_CSV_URL" download="greenmedical_flowers.csv">{{ de.app.csv }}</a>
      <a :href="EXPORT_JSON_URL" download="flowers.json">{{ de.app.json }}</a>
    </nav>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { EXPORT_CSV_URL, EXPORT_JSON_URL } from '@/api/endpoints';
import type { Metadata } from '@/api/types';
import { de } from '@/i18n/de';
import { dateTime } from '@/lib/format';
import { useNavigationStore } from '@/stores/navigation';

const props = defineProps<{ metadata: Metadata | null }>();

// Keeps the filters/sort of the overview when navigating back via the brand.
const navigation = useNavigationStore();

const updatedAt = computed(() => dateTime(props.metadata?.generated_at));
</script>
