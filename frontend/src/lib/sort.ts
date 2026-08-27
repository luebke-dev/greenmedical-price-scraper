import type { Strain } from '@/api/types';
import { de } from '@/i18n/de';
import { collator } from './format';

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

export function getSortValue(row: Strain, key: SortKey): number | string | null {
  switch (key) {
    case 'price':
      return row.sort.price;
    case 'price_per_thc_gram':
      return row.sort.price_per_thc_gram;
    case 'thc':
      return row.sort.thc;
    case 'cbd':
      return row.sort.cbd;
    case 'pharmacy_count':
      return row.pharmacy_count;
    case 'rating':
      return row.sort.rating ?? null;
    default:
      return row[key] || '';
  }
}

function compareNumbers(left: number | null, right: number | null, direction: 1 | -1): number {
  // Deliberate deviation from app.js: rows without a value go last in BOTH directions.
  if (left === null && right === null) return 0;
  if (left === null) return 1;
  if (right === null) return -1;
  return (left - right) * direction;
}

export function compareRows(left: Strain, right: Strain, sort: SortState): number {
  const column = columnDef(sort.key);
  const direction = sort.direction === 'asc' ? 1 : -1;
  const leftValue = getSortValue(left, sort.key);
  const rightValue = getSortValue(right, sort.key);

  if (column.type === 'number') {
    return compareNumbers(
      typeof leftValue === 'number' ? leftValue : null,
      typeof rightValue === 'number' ? rightValue : null,
      direction,
    );
  }
  return collator.compare(String(leftValue ?? ''), String(rightValue ?? '')) * direction;
}

/** Returns a new, stably sorted array. */
export function sortRows(rows: readonly Strain[], sort: SortState): Strain[] {
  return [...rows].sort((left, right) => compareRows(left, right, sort));
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
