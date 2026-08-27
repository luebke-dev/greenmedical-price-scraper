<template>
  <q-layout view="hHh lpR fFf">
    <q-page-container>
      <main class="page">
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
import { de } from '@/i18n/de';
import { useCatalogStore } from '@/stores/catalog';

const catalog = useCatalogStore();

onMounted(() => {
  void catalog.load();
});
</script>
