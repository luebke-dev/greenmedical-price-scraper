// Sort state of the overview table. Sorting itself happens on the server (GET /strains?sort&dir).

import { de } from '@/i18n/de';

export type SortKey =
  | 'name'
  | 'bezeichnung'
  | 'price'
  | 'price_per_thc_gram'
  | 'thc'
  | 'cbd'
  | 'genetik'
  | 'pharmacy_count'
  | 'rating';

export type SortDirection = 'asc' | 'desc';

export interface SortState {
  key: SortKey;
  direction: SortDirection;
}

export interface ColumnDef {
  key: SortKey;
  label: string;
  type: 'text' | 'number';
  className?: string;
  width: string;
  /** Direction used when the column is first selected (default: asc). */
  defaultDirection?: SortDirection;
}

/** The 9 table columns, in display order (the 8 of site/app.js plus „Bewertung“). */
export const COLUMNS: readonly ColumnDef[] = [
  { key: 'name', label: de.table.columns.name, type: 'text', width: '18%' },
  { key: 'bezeichnung', label: de.table.columns.bezeichnung, type: 'text', width: '17%' },
  { key: 'price', label: de.table.columns.price, type: 'number', className: 'price', width: '11%' },
  {
    key: 'price_per_thc_gram',
    label: de.table.columns.price_per_thc_gram,
    type: 'number',
    className: 'price',
    width: '12%',
  },
  { key: 'thc', label: de.table.columns.thc, type: 'number', width: '7%' },
  { key: 'cbd', label: de.table.columns.cbd, type: 'number', width: '7%' },
  { key: 'genetik', label: de.table.columns.genetik, type: 'text', width: '11%' },
  { key: 'pharmacy_count', label: de.table.columns.pharmacy_count, type: 'number', width: '8%' },
  {
    key: 'rating',
    label: de.table.columns.rating,
    type: 'number',
    className: 'rating',
    width: '9%',
    defaultDirection: 'desc',
  },
];

export const DEFAULT_SORT: Readonly<SortState> = { key: 'price', direction: 'asc' };

const SORT_KEYS = new Set<string>(COLUMNS.map((column) => column.key));

export function isSortKey(value: unknown): value is SortKey {
  return typeof value === 'string' && SORT_KEYS.has(value);
}

export function isSortDirection(value: unknown): value is SortDirection {
  return value === 'asc' || value === 'desc';
}

export function columnDef(key: SortKey): ColumnDef {
  const column = COLUMNS.find((item) => item.key === key);
  if (!column) throw new Error(`unknown column: ${key}`);
  return column;
}

/** Same column → flip direction; other column → its default direction (ascending). */
export function toggleSort(current: SortState, key: SortKey): SortState {
  if (current.key === key) {
    return { key, direction: current.direction === 'asc' ? 'desc' : 'asc' };
  }
  return { key, direction: columnDef(key).defaultDirection ?? 'asc' };
}

export type AriaSort = 'ascending' | 'descending' | 'none';

export function ariaSort(current: SortState, key: SortKey): AriaSort {
  if (current.key !== key) return 'none';
  return current.direction === 'asc' ? 'ascending' : 'descending';
}

export function isDefaultSort(sort: SortState): boolean {
  return sort.key === DEFAULT_SORT.key && sort.direction === DEFAULT_SORT.direction;
}
