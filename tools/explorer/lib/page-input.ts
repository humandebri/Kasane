// Server functions are directly callable; validate their arguments before DB/RPC work.
export function validatePageInput<T extends object>(
  input: T,
  required: readonly string[] = [],
  repeated: readonly string[] = [],
): T {
  if (!input || typeof input !== "object" || Array.isArray(input)) throw new TypeError("invalid page input");
  const fields = new Map(Object.entries(input));
  for (const key of required) {
    if (typeof fields.get(key) !== "string") throw new TypeError(`invalid ${key}`);
  }
  for (const [key, value] of fields) {
    if (value === undefined || typeof value === "string") continue;
    if (repeated.includes(key) && Array.isArray(value) && value.every((item) => typeof item === "string")) continue;
    throw new TypeError(`invalid ${key}`);
  }
  return input;
}
