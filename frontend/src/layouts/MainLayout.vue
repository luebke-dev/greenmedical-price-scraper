<template>
  <q-layout view="hHh lpR fFf">
    <q-page-container>
      <main class="page">
        <RefreshBanner
          :phase="schedule.phase.value"
          :text="schedule.text.value"
          :updated="schedule.updated.value"
        />
        <header class="page-header">
          <AppHeader :metadata="catalog.metadata" />
        </header>

        <router-view v-slot="{ Component }">
          <keep-alive include="IndexPage">
            <component :is="Component" />
          </keep-alive>
        </router-view>

        <footer class="page-footer">{{ de.app.footer }}</footer>
      </main>
    </q-page-container>
  </q-layout>
</template>

<script setup lang="ts">
import { onMounted } from 'vue';
import AppHeader from '@/components/AppHeader.vue';
import RefreshBanner from '@/components/RefreshBanner.vue';
import { useRefreshSchedule } from '@/composables/useRefreshSchedule';
import { de } from '@/i18n/de';
import { useCatalogStore } from '@/stores/catalog';

const catalog = useCatalogStore();
const schedule = useRefreshSchedule();

onMounted(() => {
  void catalog.load();
});
</script>
