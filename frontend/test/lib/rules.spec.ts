import { describe, expect, it } from 'vitest';
import {
  draftsFromRules,
  draftSummary,
  isValidEmail,
  makeDraft,
  ruleSummary,
  toRuleInput,
  validateDrafts,
} from '@/lib/rules';
import { makeRule } from '../fixtures';

describe('ruleSummary', () => {
  it('renders one readable line per kind', () => {
    const strain = { strain_name: 'OG Kush' };
    expect(ruleSummary({ kind: 'strain_available', ...strain })).toBe(
      'OG Kush ist wieder verfügbar',
    );
    expect(ruleSummary({ kind: 'strain_price_below', threshold: 6, ...strain })).toBe(
      'Preis von OG Kush fällt unter 6,00 €/g',
    );
    expect(ruleSummary({ kind: 'any_price_below', threshold: 4.5 })).toBe(
      'Preis irgendeiner Sorte fällt unter 4,50 €/g',
    );
    expect(ruleSummary({ kind: 'thc_above', threshold: 25 })).toBe(
      'Neue Sorte mit mehr als 25,0 % THC',
    );
    expect(ruleSummary({ kind: 'new_strain' })).toBe('Jede neue Sorte');
    expect(ruleSummary({ kind: 'strain_price_change', ...strain })).toBe(
      'Preis von OG Kush ändert sich',
    );
  });

  it('falls back for a missing strain name or threshold', () => {
    expect(ruleSummary({ kind: 'strain_available', strain_name: null })).toBe(
      'unbekannte Sorte ist wieder verfügbar',
    );
    expect(ruleSummary({ kind: 'any_price_below' })).toBe('Preis irgendeiner Sorte fällt unter –');
  });

  it('summarises editor drafts', () => {
    const draft = makeDraft({
      kind: 'strain_price_below',
      strain: { id: 1, name: 'Gelato', bezeichnung: '' },
      threshold: 7.25,
    });
    expect(draftSummary(draft)).toBe('Preis von Gelato fällt unter 7,25 €/g');
  });
});

describe('drafts', () => {
  it('converts drafts to rule inputs and drops unused fields', () => {
    const strain = { id: 3, name: 'X', bezeichnung: '' };
    expect(toRuleInput(makeDraft({ kind: 'new_strain', strain, threshold: 5 }))).toEqual({
      kind: 'new_strain',
    });
    expect(
      toRuleInput(makeDraft({ kind: 'strain_price_below', strain, threshold: 5.999 })),
    ).toEqual({ kind: 'strain_price_below', strain_id: 3, threshold: 6 });
    expect(toRuleInput(makeDraft({ kind: 'thc_above', threshold: 20 }))).toEqual({
      kind: 'thc_above',
      threshold: 20,
    });
  });

  it('builds drafts from stored rules', () => {
    const drafts = draftsFromRules([makeRule(), makeRule({ id: 2, kind: 'new_strain' })]);
    expect(drafts).toHaveLength(2);
    expect(drafts[0]).toMatchObject({
      kind: 'strain_price_below',
      strain: { id: 7, name: 'OG Kush' },
      threshold: 6,
    });
    expect(drafts[0]!.key).not.toBe(drafts[1]!.key);
  });

  it('validates required fields and duplicates', () => {
    const strain = { id: 3, name: 'X', bezeichnung: '' };
    const missingStrain = makeDraft({ kind: 'strain_available' });
    const badThreshold = makeDraft({ kind: 'any_price_below', threshold: 0 });
    const a = makeDraft({ kind: 'strain_price_below', strain, threshold: 5 });
    const b = makeDraft({ kind: 'strain_price_below', strain, threshold: 5 });
    const ok = makeDraft({ kind: 'new_strain' });
    const errors = validateDrafts([missingStrain, badThreshold, a, b, ok]);
    expect(errors.get(missingStrain.key)).toEqual({ strain: 'Bitte eine Sorte wählen.' });
    expect(errors.get(badThreshold.key)).toEqual({
      threshold: 'Bitte einen Schwellwert größer 0 eingeben.',
    });
    expect(errors.has(a.key)).toBe(false);
    expect(errors.get(b.key)).toEqual({ duplicate: 'Diese Regel ist doppelt.' });
    expect(errors.has(ok.key)).toBe(false);
  });
});

describe('isValidEmail', () => {
  it('accepts plain addresses and rejects junk', () => {
    expect(isValidEmail('a@b.de')).toBe(true);
    expect(isValidEmail('  name+tag@example.org ')).toBe(true);
    expect(isValidEmail('')).toBe(false);
    expect(isValidEmail('nope')).toBe(false);
    expect(isValidEmail('a@b')).toBe(false);
    expect(isValidEmail('a b@c.de')).toBe(false);
  });
});
