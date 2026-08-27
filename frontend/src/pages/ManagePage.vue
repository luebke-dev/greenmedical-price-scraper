<template>
  <q-page class="manage-page">
    <nav class="back-nav">
      <router-link class="back-link" :to="{ name: 'index' }">{{ de.confirmPage.home }}</router-link>
    </nav>
    <section class="facts form-card" :aria-labelledby="headingId">
      <h2 :id="headingId" class="facts-name">{{ de.manage.heading }}</h2>

      <template v-if="store.deleted">
        <p class="success" role="status">{{ de.manage.unsubscribed }}</p>
        <p class="field-hint">{{ de.manage.unsubscribedHint }}</p>
        <div class="form-actions">
          <router-link class="back-link" :to="{ name: 'subscribe' }">{{
            de.manage.newSubscription
          }}</router-link>
        </div>
      </template>
      <EmptyState v-else-if="!token" :message="de.manage.missingToken" tone="error" />
      <EmptyState
        v-else-if="store.error && !store.subscription"
        :message="store.error.message"
        tone="error"
      />
      <EmptyState v-else-if="!store.subscription" :message="de.manage.loading" />
      <form v-else class="subscribe-form" novalidate @submit.prevent="save">
        <p class="facts-sub">{{ de.manage.email(store.subscription.email) }}</p>
        <p v-if="!store.subscription.confirmed" class="notice" role="status">
          {{ de.manage.unconfirmed }}
        </p>

        <RuleEditor
          v-model="drafts"
          :errors="errors"
          :list-error="listError"
          :disabled="store.loading"
        />

        <p v-if="store.error" class="field-error form-error" role="alert">
          {{ store.error.message }}
        </p>
        <p v-else-if="saved" class="success" role="status">{{ de.manage.saved }}</p>

        <div class="form-actions">
          <button type="submit" class="clear-button primary-button" :disabled="store.loading">
            {{ store.loading ? de.manage.saving : de.manage.save }}
          </button>
          <button
            type="button"
            class="clear-button danger-button"
            :disabled="store.loading"
            @click="dialogOpen = true"
          >
            {{ de.manage.unsubscribe }}
          </button>
        </div>
      </form>
    </section>

    <q-dialog v-model="dialogOpen" role="alertdialog" :aria-label="de.manage.unsubscribeTitle">
      <div class="facts dialog-card">
        <h3 class="form-subheading">{{ de.manage.unsubscribeTitle }}</h3>
        <p>{{ de.manage.unsubscribeText }}</p>
        <div class="form-actions">
          <button type="button" class="clear-button" @click="dialogOpen = false">
            {{ de.manage.cancel }}
          </button>
          <button type="button" class="clear-button danger-button" @click="unsubscribe">
            {{ de.manage.unsubscribeConfirm }}
          </button>
        </div>
      </div>
    </q-dialog>
  </q-page>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, shallowRef, watch } from 'vue';
import { useRoute } from 'vue-router';
import EmptyState from '@/components/EmptyState.vue';
import RuleEditor from '@/components/RuleEditor.vue';
import { de } from '@/i18n/de';
import {
  draftsFromRules,
  toRuleInput,
  validateDrafts,
  type DraftErrors,
  type RuleDraft,
} from '@/lib/rules';
import { useSubscriptionsStore } from '@/stores/subscriptions';

defineOptions({ name: 'ManagePage' });

const headingId = 'manage-heading';
const route = useRoute();
const store = useSubscriptionsStore();

const drafts = ref<RuleDraft[]>([]);
const errors = shallowRef<Map<number, DraftErrors>>(new Map());
const listError = ref<string | null>(null);
const saved = ref(false);
const dialogOpen = ref(false);

const token = computed(() => {
  const raw = route.query.token;
  const value = Array.isArray(raw) ? raw[0] : raw;
  return typeof value === 'string' && value.trim() ? value.trim() : null;
});

store.reset();
watch(
  token,
  async (value) => {
    if (!value) return;
    const subscription = await store.load(value);
    if (subscription) drafts.value = draftsFromRules(subscription.rules);
  },
  { immediate: true },
);
onBeforeUnmount(() => store.reset());

watch(drafts, () => {
  saved.value = false;
  if (errors.value.size > 0) errors.value = validateDrafts(drafts.value);
  if (listError.value && drafts.value.length > 0) listError.value = null;
});

async function save(): Promise<void> {
  if (!token.value || store.loading) return;
  errors.value = validateDrafts(drafts.value);
  listError.value = drafts.value.length === 0 ? de.rules.editor.errors.empty : null;
  if (errors.value.size > 0 || listError.value) return;
  const result = await store.update(token.value, drafts.value.map(toRuleInput));
  if (result) {
    drafts.value = draftsFromRules(result.rules);
    // The drafts watcher (which clears `saved`) runs before the next render.
    await nextTick();
    saved.value = true;
  }
}

async function unsubscribe(): Promise<void> {
  dialogOpen.value = false;
  if (!token.value) return;
  await store.remove(token.value);
}
</script>
