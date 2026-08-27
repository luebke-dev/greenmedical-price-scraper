<template>
  <q-page class="not-found">
    <div class="not-found-code" aria-hidden="true">{{ de.notFound.code }}</div>
    <h2 class="not-found-text">{{ de.notFound.text }}</h2>
    <router-link class="back-link" :to="{ name: 'index' }">{{ de.app.home }}</router-link>
  </q-page>
</template>

<script setup lang="ts">
import { de } from '@/i18n/de';
</script>
