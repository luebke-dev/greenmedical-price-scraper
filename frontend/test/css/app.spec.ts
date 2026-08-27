// Regression checks on the compiled global stylesheet (vitest compiles src/css/*.scss, see
// vitest.config.ts). These guard rules that fight Quasar's own defaults and only show up
// visually, e.g. cell wrapping in the fixed-layout strain table.
import { describe, expect, it } from 'vitest';
import css from '@/css/app.scss?inline';

/** Merged declarations of every top-level rule whose selector list contains `selector`. */
function declarations(stylesheet: string, selector: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const match of stylesheet.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const selectors = (match[1] ?? '').split(',').map((item) => item.trim());
    if (!selectors.includes(selector)) continue;
    for (const declaration of (match[2] ?? '').split(';')) {
      const colon = declaration.indexOf(':');
      if (colon === -1) continue;
      result[declaration.slice(0, colon).trim()] = declaration.slice(colon + 1).trim();
    }
  }
  return result;
}

describe('app.scss', () => {
  it('compiles', () => {
    expect(css.length).toBeGreaterThan(1000);
  });

  it('lets strain table cells wrap (overrides Quasar’s `.q-table td { white-space: nowrap }`)', () => {
    const cell = declarations(css, '.strain-table .q-table td');
    expect(cell['white-space']).toBe('normal');
    expect(cell['overflow-wrap']).toBe('anywhere');
    // The table keeps its fixed layout, so wrapping is the only way long cells fit.
    expect(declarations(css, '.strain-table table.q-table')['table-layout']).toBe('fixed');
  });

  it('lets the offers sub-table cells wrap as well', () => {
    const cell = declarations(css, 'table.offers td');
    expect(cell['white-space']).toBe('normal');
    expect(cell['overflow-wrap']).toBe('anywhere');
  });

  it('keeps price, status and buy cells on one line', () => {
    expect(declarations(css, '.price')['white-space']).toBe('nowrap');
    expect(declarations(css, '.status')['white-space']).toBe('nowrap');
    expect(declarations(css, '.buy-cell')['white-space']).toBe('nowrap');
  });
});
