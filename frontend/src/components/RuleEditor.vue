<template>
  <fieldset class="rule-editor" :disabled="disabled">
    <legend class="rule-editor-legend">{{ de.rules.editor.heading }}</legend>
    <p v-if="listError" class="field-error" role="alert">{{ listError }}</p>
    <ol class="rule-list">
      <li
        v-for="(draft, index) in modelValue"
        :key="draft.key"
        class="rule-row filter"
        :aria-label="de.rules.editor.rule(index + 1)"
      >
        <div class="rule-row-head">
          <span class="filter-label">{{ de.rules.editor.rule(index + 1) }}</span>
          <button
            type="button"
            class="clear-button rule-remove"
            :aria-label="de.rules.editor.removeAria(index + 1)"
            :disabled="modelValue.length <= 1"
            @click="remove(index)"
          >
            {{ de.rules.editor.remove }}
          </button>
        </div>
        <div class="rule-fields">
          <label class="rule-field rule-field-kind">
            <span class="field-label">{{ de.rules.editor.kind }}</span>
            <select
              class="pager-size-select field-select"
              :value="draft.kind"
              @change="onKind(index, $event)"
            >
              <option v-for="kind in RULE_KINDS" :key="kind" :value="kind">
                {{ de.rules.kinds[kind].label }}
              </option>
            </select>
            <span class="field-hint">{{ de.rules.kinds[draft.kind].hint }}</span>
          </label>

          <div v-if="RULE_KIND_META[draft.kind].strain" class="rule-field rule-field-strain">
            <span :id="`rule-${draft.key}-strain-label`" class="field-label">{{
              de.rules.editor.strain
            }}</span>
            <q-select
              class="field q-select-strain"
              :model-value="draft.strain"
              :options="strainOptions"
              :loading="strainLoading"
              use-input
              fill-input
              hide-selected
              borderless
              dense
              hide-bottom-space
              :input-debounce="STRAIN_DEBOUNCE_MS"
              option-value="id"
              option-label="name"
              :placeholder="draft.strain ? undefined : de.rules.editor.strainPlaceholder"
              :aria-labelledby="`rule-${draft.key}-strain-label`"
              :error="Boolean(errors.get(draft.key)?.strain)"
              @filter="filterStrains"
              @update:model-value="(value) => patch(index, { strain: value })"
            >
              <template #option="scope">
                <q-item v-bind="scope.itemProps">
                  <q-item-section>
                    <q-item-label>{{ scope.opt.name }}</q-item-label>
                    <q-item-label v-if="scope.opt.bezeichnung" caption>{{
                      scope.opt.bezeichnung
                    }}</q-item-label>
                  </q-item-section>
                </q-item>
              </template>
              <template #no-option>
                <q-item>
                  <q-item-section class="text-muted">{{ strainEmptyText }}</q-item-section>
                </q-item>
              </template>
            </q-select>
            <span v-if="draft.strain?.bezeichnung" class="field-hint">{{
              draft.strain.bezeichnung
            }}</span>
            <span v-if="errors.get(draft.key)?.strain" class="field-error" role="alert">{{
              errors.get(draft.key)?.strain
            }}</span>
          </div>

          <label
            v-if="RULE_KIND_META[draft.kind].threshold"
            class="rule-field rule-field-threshold"
          >
            <span class="field-label">{{ de.rules.editor.threshold }}</span>
            <q-input
              class="field"
              :model-value="draft.threshold"
              type="number"
              inputmode="decimal"
              min="0"
              :step="RULE_KIND_META[draft.kind].threshold === 'euro' ? '0.01' : '0.1'"
              :suffix="
                RULE_KIND_META[draft.kind].threshold === 'euro'
                  ? de.rules.editor.thresholdEuro
                  : de.rules.editor.thresholdPercent
              "
              borderless
              dense
              hide-bottom-space
              :error="Boolean(errors.get(draft.key)?.threshold)"
              @update:model-value="(value) => patch(index, { threshold: toNumber(value) })"
            />
            <span v-if="errors.get(draft.key)?.threshold" class="field-error" role="alert">{{
              errors.get(draft.key)?.threshold
            }}</span>
          </label>
        </div>
        <span v-if="errors.get(draft.key)?.duplicate" class="field-error" role="alert">{{
          errors.get(draft.key)?.duplicate
        }}</span>
      </li>
    </ol>
    <div class="rule-editor-actions">
      <button
        type="button"
        class="clear-button rule-add"
        :disabled="modelValue.length >= MAX_RULES"
        @click="add"
      >
        {{ de.rules.editor.add }}
      </button>
      <span v-if="modelValue.length >= MAX_RULES" class="field-hint">{{
        de.rules.editor.max(MAX_RULES)
      }}</span>
    </div>
  </fieldset>
</template>

<script setup lang="ts">
import { computed, ref, shallowRef } from 'vue';
import { isAbortError } from '@/api/client';
import { getStrains } from '@/api/endpoints';
import { de } from '@/i18n/de';
import {
  MAX_RULES,
  RULE_KINDS,
  RULE_KIND_META,
  STRAIN_DEBOUNCE_MS,
  STRAIN_MIN_CHARS,
  isRuleKind,
  makeDraft,
  type DraftErrors,
  type RuleDraft,
  type StrainOption,
} from '@/lib/rules';

const props = withDefaults(
  defineProps<{
    modelValue: RuleDraft[];
    errors?: Map<number, DraftErrors>;
    /** Message for the whole list (e.g. "Mindestens eine Regel"). */
    listError?: string | null;
    disabled?: boolean;
  }>(),
  { errors: () => new Map(), listError: null, disabled: false },
);

const emit = defineEmits<{ 'update:modelValue': [drafts: RuleDraft[]] }>();

const strainOptions = shallowRef<StrainOption[]>([]);
const strainLoading = ref(false);
const lastQuery = ref('');
let strainController: AbortController | null = null;

const strainEmptyText = computed(() =>
  lastQuery.value.length < STRAIN_MIN_CHARS
    ? de.rules.editor.strainMinChars
    : de.rules.editor.strainNoResults,
);

function toNumber(value: string | number | null): number | null {
  if (value === null || value === '') return null;
  const parsed = typeof value === 'number' ? value : Number(String(value).replace(',', '.'));
  return Number.isFinite(parsed) ? parsed : null;
}

function patch(index: number, changes: Partial<RuleDraft>): void {
  const next = props.modelValue.map((draft, i) => (i === index ? { ...draft, ...changes } : draft));
  emit('update:modelValue', next);
}

function onKind(index: number, event: Event): void {
  const value = (event.target as HTMLSelectElement).value;
  if (!isRuleKind(value)) return;
  const meta = RULE_KIND_META[value];
  const current = props.modelValue[index];
  patch(index, {
    kind: value,
    strain: meta.strain ? (current?.strain ?? null) : null,
    threshold: meta.threshold ? (current?.threshold ?? null) : null,
  });
}

function add(): void {
  if (props.modelValue.length >= MAX_RULES) return;
  emit('update:modelValue', [...props.modelValue, makeDraft()]);
}

function remove(index: number): void {
  if (props.modelValue.length <= 1) return;
  emit(
    'update:modelValue',
    props.modelValue.filter((_, i) => i !== index),
  );
}

/** QSelect filter hook (already debounced via `input-debounce`). */
function filterStrains(input: string, done: (update: () => void) => void, abort: () => void): void {
  const query = input.trim();
  lastQuery.value = query;
  strainController?.abort();
  if (query.length < STRAIN_MIN_CHARS) {
    done(() => {
      strainOptions.value = [];
    });
    return;
  }
  strainController = new AbortController();
  const signal = strainController.signal;
  strainLoading.value = true;
  getStrains({ q: query, limit: 10, sort: 'name' }, signal)
    .then((page) => {
      if (signal.aborted) return;
      done(() => {
        strainOptions.value = page.strains.map((strain) => ({
          id: strain.id,
          name: strain.name,
          bezeichnung: strain.bezeichnung,
        }));
      });
    })
    .catch((cause: unknown) => {
      if (signal.aborted || isAbortError(cause)) return;
      abort();
    })
    .finally(() => {
      if (!signal.aborted) strainLoading.value = false;
    });
}
</script>
