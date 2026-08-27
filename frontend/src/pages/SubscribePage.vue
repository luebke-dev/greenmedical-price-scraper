<template>
  <q-page class="subscribe-page">
    <nav class="back-nav">
      <router-link class="back-link" :to="navigation.indexLocation">{{
        de.strain.back
      }}</router-link>
    </nav>

    <section class="facts form-card" :aria-labelledby="headingId">
      <h2 :id="headingId" class="facts-name">{{ de.subscribe.heading }}</h2>

      <template v-if="store.created">
        <p class="success" role="status">{{ de.subscribe.successTitle }}</p>
        <p class="field-hint">{{ de.subscribe.successHint }}</p>
        <div class="form-actions">
          <button type="button" class="clear-button" @click="startOver">
            {{ de.subscribe.another }}
          </button>
        </div>
      </template>

      <form v-else class="subscribe-form" novalidate @submit.prevent="submit">
        <p class="facts-sub">{{ de.subscribe.intro }}</p>
        <p v-if="prefillState === 'loading'" class="field-hint" role="status">
          {{ de.subscribe.prefillLoading }}
        </p>
        <p v-else-if="prefillState === 'error'" class="notice" role="status">
          {{ de.subscribe.prefillError }}
        </p>
        <p v-else-if="prefillName" class="notice" role="status">
          {{ de.subscribe.prefill(prefillName) }}
        </p>

        <label class="rule-field email-field">
          <span class="field-label">{{ de.subscribe.email }}</span>
          <q-input
            class="field"
            :model-value="email"
            type="email"
            name="email"
            autocomplete="email"
            inputmode="email"
            borderless
            dense
            hide-bottom-space
            :placeholder="de.subscribe.emailPlaceholder"
            :error="Boolean(emailError)"
            @update:model-value="(value) => (email = value === null ? '' : String(value))"
          />
          <span v-if="emailError" class="field-error" role="alert">{{ emailError }}</span>
        </label>

        <!-- Honeypot: invisible for people, filled in by bots. -->
        <div class="hp" aria-hidden="true">
          <label>
            Website
            <input v-model="website" type="text" name="website" tabindex="-1" autocomplete="off" />
          </label>
        </div>

        <RuleEditor
          v-model="drafts"
          :errors="errors"
          :list-error="listError"
          :disabled="store.loading"
        />

        <p v-if="store.error" class="field-error form-error" role="alert">
          {{ store.error.message }}
        </p>

        <div class="form-actions">
          <button type="submit" class="clear-button primary-button" :disabled="store.loading">
            {{ store.loading ? de.subscribe.submitting : de.subscribe.submit }}
          </button>
        </div>
      </form>
    </section>
  </q-page>
</template>

<script setup lang="ts">
import { onBeforeUnmount, ref, shallowRef, watch } from 'vue';
import { useRoute } from 'vue-router';
import { getStrain } from '@/api/endpoints';
import RuleEditor from '@/components/RuleEditor.vue';
import { de } from '@/i18n/de';
import {
  isValidEmail,
  makeDraft,
  toRuleInput,
  validateDrafts,
  type DraftErrors,
  type RuleDraft,
} from '@/lib/rules';
import { useNavigationStore } from '@/stores/navigation';
import { useSubscriptionsStore } from '@/stores/subscriptions';

defineOptions({ name: 'SubscribePage' });

const headingId = 'subscribe-heading';

const route = useRoute();
const navigation = useNavigationStore();
const store = useSubscriptionsStore();

const email = ref('');
const website = ref('');
const drafts = ref<RuleDraft[]>([makeDraft()]);
const errors = shallowRef<Map<number, DraftErrors>>(new Map());
const listError = ref<string | null>(null);
const emailError = ref<string | null>(null);
const prefillState = ref<'idle' | 'loading' | 'done' | 'error'>('idle');
const prefillName = ref<string | null>(null);

let prefillController: AbortController | null = null;

function queryStrainId(): number | null {
  const raw = route.query.strain_id;
  const value = Array.isArray(raw) ? raw[0] : raw;
  if (typeof value !== 'string' || !/^\d+$/.test(value)) return null;
  const id = Number(value);
  return id > 0 ? id : null;
}

/** `?strain_id=` prefills "wieder verfügbar" + "Preis unter …" for that strain. */
async function prefill(id: number): Promise<void> {
  prefillController?.abort();
  prefillController = new AbortController();
  const signal = prefillController.signal;
  prefillState.value = 'loading';
  try {
    const detail = await getStrain(id, signal);
    if (signal.aborted) return;
    const strain = { id: detail.id, name: detail.name, bezeichnung: detail.bezeichnung };
    drafts.value = [
      makeDraft({ kind: 'strain_available', strain }),
      makeDraft({ kind: 'strain_price_below', strain, threshold: detail.min_price ?? null }),
    ];
    prefillName.value = detail.name;
    prefillState.value = 'done';
  } catch {
    if (signal.aborted) return;
    prefillState.value = 'error';
  }
}

watch(
  () => route.query.strain_id,
  () => {
    const id = queryStrainId();
    if (id !== null) void prefill(id);
  },
  { immediate: true },
);

store.reset();
onBeforeUnmount(() => {
  prefillController?.abort();
  store.reset();
});

// Clear the field errors as soon as the user fixes them.
watch(email, () => {
  if (emailError.value && isValidEmail(email.value)) emailError.value = null;
});
watch(drafts, () => {
  if (errors.value.size > 0) errors.value = validateDrafts(drafts.value);
  if (listError.value && drafts.value.length > 0) listError.value = null;
});

function validate(): boolean {
  emailError.value = isValidEmail(email.value) ? null : de.subscribe.emailInvalid;
  errors.value = validateDrafts(drafts.value);
  listError.value = drafts.value.length === 0 ? de.rules.editor.errors.empty : null;
  return !emailError.value && errors.value.size === 0 && !listError.value;
}

async function submit(): Promise<void> {
  if (store.loading || !validate()) return;
  await store.create({
    email: email.value.trim(),
    rules: drafts.value.map(toRuleInput),
    website: website.value,
  });
}

function startOver(): void {
  store.reset();
  drafts.value = [makeDraft()];
  prefillName.value = null;
  prefillState.value = 'idle';
}
</script>
