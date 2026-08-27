// Substring search over the precomputed, lowercased `search` field.

export interface Searchable {
  search: string;
}

export function normalizeQuery(query: string): string {
  return query.trim().toLowerCase();
}

export function matchesSearch(row: Searchable, query: string): boolean {
  const normalized = normalizeQuery(query);
  return normalized === '' || row.search.includes(normalized);
}
