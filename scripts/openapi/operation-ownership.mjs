const OWNERSHIP_KEYS = Object.freeze({
  core: "core",
  hostedExtension: "hosted-extension",
  internal: "internal",
});

function operationSet(document) {
  const identifiers = [];
  for (const item of Object.values(document.paths ?? {})) {
    for (const operation of Object.values(item)) {
      if (typeof operation?.operationId === "string") identifiers.push(operation.operationId);
    }
  }
  return identifiers.toSorted((left, right) => left.localeCompare(right));
}

function classifiedOwnership(ownership) {
  if (ownership?.schemaVersion !== 1)
    throw new Error("operation ownership schema must be version 1");
  const classified = new Map();
  for (const [key, label] of Object.entries(OWNERSHIP_KEYS)) {
    const identifiers = ownership[key];
    if (!Array.isArray(identifiers)) throw new Error(`${key} ownership must be an array`);
    for (const identifier of identifiers) {
      if (typeof identifier !== "string" || identifier.length === 0) {
        throw new Error(`${key} ownership contains an invalid operation identifier`);
      }
      if (classified.has(identifier)) throw new Error(`${identifier} has duplicate ownership`);
      classified.set(identifier, label);
    }
  }
  return classified;
}

export function operationOwnership(document, ownership) {
  const operations = operationSet(document);
  const classified = classifiedOwnership(ownership);
  const missing = operations.filter((identifier) => !classified.has(identifier));
  const stale = [...classified.keys()].filter((identifier) => !operations.includes(identifier));
  if (missing.length > 0) throw new Error(`operations missing ownership: ${missing.join(", ")}`);
  if (stale.length > 0)
    throw new Error(`ownership contains unknown operations: ${stale.join(", ")}`);
  return classified;
}

export function operationOwnershipManifest(items) {
  return {
    operations: items.map(({ id, method, ownership, path }) => ({ id, method, ownership, path })),
    schemaVersion: 1,
  };
}
