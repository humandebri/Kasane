// Keep decimal strings and repeated query parameters intact.
export function parseUrlSearch(search: string): Record<string, string | string[]> {
  const values = new Map<string, string | string[]>();
  for (const [key, value] of new URLSearchParams(search)) {
    const previous = values.get(key);
    values.set(key, previous === undefined ? value : Array.isArray(previous) ? [...previous, value] : [previous, value]);
  }
  return Object.fromEntries(values);
}
export function stringifyUrlSearch(search: Record<string, unknown>): string {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(search)) {
    if (value === undefined) continue;
    for (const item of Array.isArray(value) ? value : [value]) {
      if (typeof item === "string" || typeof item === "number" || typeof item === "boolean") query.append(key, String(item));
    }
  }
  return query.size ? `?${query}` : "";
}
export function searchString(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}
export function searchStrings(value: unknown): string | string[] | undefined {
  return typeof value === "string" || (Array.isArray(value) && value.every((item): item is string => typeof item === "string")) ? value : undefined;
}
