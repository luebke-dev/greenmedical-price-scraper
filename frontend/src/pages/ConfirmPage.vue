<template>
  <q-page class="confirm-page">
    <nav class="back-nav">
      <router-link class="back-link" :to="{ name: 'index' }">{{ de.confirmPage.home }}</router-link>
    </nav>
    <section class="facts form-card" :aria-labelledby="headingId">
      <h2 :id="headingId" class="facts-name">{{ de.confirmPage.heading }}</h2>

      <EmptyState v-if="!token" :message="de.confirmPage.missingToken" tone="error" />
      <EmptyState v-else-if="store.error" :message="store.error.message" tone="error" />
      <EmptyState v-else-if="!store.subscription" :message="de.confirmPage.loading" />
      <template v-else>
        <p class="success" role="status">{{ de.confirmPage.success(store.subscription.email) }}</p>
        <h3 class="form-subheading">{{ de.confirmPage.rules }}</h3>
        <RuleSummary :rules="store.subscription.rules" />
        <p class="field-hint">{{ de.confirmPage.manageHint }}</p>
        <div class="form-actions">
          <router-link class="back-link" :to="{ name: 'subscribe' }">{{
            de.subscribe.another
          }}</router-link>
        </div>
      </template>
    </section>
  </q-page>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, watch } from 'vue';
import { useRoute } from 'vue-router';
import EmptyState from '@/components/EmptyState.vue';
import RuleSummary from '@/components/RuleSummary.vue';
import { de } from '@/i18n/de';
import { useSubscriptionsStore } from '@/stores/subscriptions';

defineOptions({ name: 'ConfirmPage' });

const headingId = 'confirm-heading';
const route = useRoute();
const store = useSubscriptionsStore();

const token = computed(() => {
  const raw = route.query.token;
  const value = Array.isArray(raw) ? raw[0] : raw;
  return typeof value === 'string' && value.trim() ? value.trim() : null;
});

store.reset();
watch(
  token,
  (value) => {
    if (value) void store.confirm(value);
  },
  { immediate: true },
);
onBeforeUnmount(() => store.reset());
</script>
