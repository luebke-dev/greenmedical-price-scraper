// Rule kinds of the price alert subscription: which fields each kind needs, editor drafts,
// validation and the readable one-line summary.

import type { Rule, RuleInput, RuleKind } from '@/api/types';
import { de } from '@/i18n/de';
import { num } from '@/lib/format';

export const RULE_KINDS: readonly RuleKind[] = [
  'strain_available',
  'strain_price_below',
  'any_price_below',
  'thc_above',
  'new_strain',
  'strain_price_change',
];

export const MAX_RULES = 20;
/** Autocomplete: debounce of the q-select input and minimum query length. */
export const STRAIN_DEBOUNCE_MS = 250;
export const STRAIN_MIN_CHARS = 2;

export type ThresholdUnit = 'euro' | 'percent';

export interface RuleKindMeta {
  strain: boolean;
  threshold: ThresholdUnit | null;
}

export const RULE_KIND_META: Record<RuleKind, RuleKindMeta> = {
  strain_available: { strain: true, threshold: null },
  strain_price_below: { strain: true, threshold: 'euro' },
  any_price_below: { strain: false, threshold: 'euro' },
  thc_above: { strain: false, threshold: 'percent' },
  new_strain: { strain: false, threshold: null },
  strain_price_change: { strain: true, threshold: null },
};

export function isRuleKind(value: unknown): value is RuleKind {
  return typeof value === 'string' && (RULE_KINDS as readonly string[]).includes(value);
}

/** Strain as picked in the autocomplete (id + display names). */
export interface StrainOption {
  id: number;
  name: string;
  designation: string;
}

/** One editable row of the rule editor. */
export interface RuleDraft {
  /** Stable key for v-for / focus management. */
  key: number;
  kind: RuleKind;
  strain: StrainOption | null;
  threshold: number | null;
}

let draftSeq = 1;

export function makeDraft(overrides: Partial<Omit<RuleDraft, 'key'>> = {}): RuleDraft {
  return {
    key: draftSeq++,
    kind: 'strain_price_below',
    strain: null,
    threshold: null,
    ...overrides,
  };
}

/** Editor rows from the rules of a loaded subscription (name only, no designation). */
export function draftsFromRules(rules: readonly Rule[]): RuleDraft[] {
  return rules.map((rule) =>
    makeDraft({
      kind: rule.kind,
      strain:
        rule.strain_id !== undefined
          ? { id: rule.strain_id, name: rule.strain_name ?? '', designation: '' }
          : null,
      threshold: rule.threshold ?? null,
    }),
  );
}

/** Payload for the API; fields the kind does not use are dropped. */
export function toRuleInput(draft: RuleDraft): RuleInput {
  const meta = RULE_KIND_META[draft.kind];
  const input: RuleInput = { kind: draft.kind };
  if (meta.strain && draft.strain) input.strain_id = draft.strain.id;
  if (meta.threshold && draft.threshold !== null) {
    input.threshold = Math.round(draft.threshold * 100) / 100;
  }
  return input;
}

export interface DraftErrors {
  strain?: string;
  threshold?: string;
  duplicate?: string;
}

function ruleIdentity(draft: RuleDraft): string {
  const input = toRuleInput(draft);
  return `${input.kind}|${input.strain_id ?? ''}|${input.threshold ?? ''}`;
}

/** Errors per draft key; an empty map means the list is valid. */
export function validateDrafts(drafts: readonly RuleDraft[]): Map<number, DraftErrors> {
  const errors = new Map<number, DraftErrors>();
  const seen = new Set<string>();
  const messages = de.rules.editor.errors;
  for (const draft of drafts) {
    const meta = RULE_KIND_META[draft.kind];
    const entry: DraftErrors = {};
    if (meta.strain && !draft.strain) entry.strain = messages.strain;
    if (
      meta.threshold &&
      (draft.threshold === null || !Number.isFinite(draft.threshold) || draft.threshold <= 0)
    ) {
      entry.threshold = messages.threshold;
    }
    if (Object.keys(entry).length === 0) {
      const identity = ruleIdentity(draft);
      if (seen.has(identity)) entry.duplicate = messages.duplicate;
      seen.add(identity);
    }
    if (Object.keys(entry).length > 0) errors.set(draft.key, entry);
  }
  return errors;
}

export function formatThreshold(value: number | null | undefined, unit: ThresholdUnit): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '–';
  return unit === 'euro' ? `${num(value, 2)} €/g` : `${num(value, 1)} %`;
}

/** Readable one-liner, e.g. "Preis von OG Kush fällt unter 6,00 €/g". */
export interface RuleLike {
  kind: RuleKind;
  threshold?: number | null | undefined;
  strain_name?: string | null | undefined;
}

export function ruleSummary(rule: RuleLike): string {
  const strain = rule.strain_name?.trim() || de.rules.summary.unknownStrain;
  const text = de.rules.summary;
  switch (rule.kind) {
    case 'strain_available':
      return text.strain_available(strain);
    case 'strain_price_below':
      return text.strain_price_below(strain, formatThreshold(rule.threshold, 'euro'));
    case 'any_price_below':
      return text.any_price_below(formatThreshold(rule.threshold, 'euro'));
    case 'thc_above':
      return text.thc_above(formatThreshold(rule.threshold, 'percent'));
    case 'new_strain':
      return text.new_strain();
    case 'strain_price_change':
      return text.strain_price_change(strain);
  }
}

export function draftSummary(draft: RuleDraft): string {
  return ruleSummary({
    kind: draft.kind,
    threshold: draft.threshold,
    strain_name: draft.strain?.name ?? null,
  });
}

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/;

export function isValidEmail(value: string): boolean {
  const trimmed = value.trim();
  return trimmed.length <= 254 && EMAIL_RE.test(trimmed);
}
