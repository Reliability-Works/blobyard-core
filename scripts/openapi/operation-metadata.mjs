const USER_SCOPES = new Set([
  "account:delete",
  "account:export",
  "audit:read",
  "billing:manage",
  "ci:manage",
  "inbox:manage",
  "members:manage",
  "object:read",
  "object:write",
  "project:read",
  "project:write",
  "retention:manage",
  "sessions:manage",
  "share:manage",
  "tokens:manage",
  "workspace:read",
  "yard:manage",
  "yard:read",
]);
const CI_ACTIONS = new Set(["download", "share", "upload", "yard:manage"]);
const RISKS = new Set(["destructive", "read", "sensitive", "write"]);

function stringArray(value, allowed, label, operation) {
  if (
    !Array.isArray(value) ||
    new Set(value).size !== value.length ||
    value.some((item) => typeof item !== "string" || !allowed.has(item))
  ) {
    throw new Error(`${operation} has invalid ${label}`);
  }
  return [...value];
}

export function operationMetadata(operationIds, document) {
  if (document?.schemaVersion !== 1 || typeof document.operations !== "object") {
    throw new Error("operation metadata schema must be version 1");
  }
  const known = new Set(operationIds);
  const configured = Object.keys(document.operations);
  const missing = operationIds.filter((identifier) => !configured.includes(identifier));
  const stale = configured.filter((identifier) => !known.has(identifier));
  if (missing.length > 0) throw new Error(`operations missing metadata: ${missing.join(", ")}`);
  if (stale.length > 0)
    throw new Error(`metadata contains unknown operations: ${stale.join(", ")}`);

  return new Map(
    operationIds.map((identifier) => {
      const metadata = document.operations[identifier];
      if (typeof metadata !== "object" || metadata === null || Array.isArray(metadata)) {
        throw new Error(`${identifier} has invalid operation metadata`);
      }
      const keys = Object.keys(metadata).toSorted();
      if (JSON.stringify(keys) !== '["requiredCiActions","requiredUserScopes","risk"]') {
        throw new Error(`${identifier} has unsupported operation metadata fields`);
      }
      const requiredUserScopes = stringArray(
        metadata.requiredUserScopes,
        USER_SCOPES,
        "user scopes",
        identifier,
      );
      const requiredCiActions = stringArray(
        metadata.requiredCiActions,
        CI_ACTIONS,
        "CI actions",
        identifier,
      );
      if (!RISKS.has(metadata.risk)) throw new Error(`${identifier} has invalid risk class`);
      return [identifier, { requiredCiActions, requiredUserScopes, risk: metadata.risk }];
    }),
  );
}

export function operationManifest(items) {
  return {
    operations: items.map(({ contract, id, metadata, method, ownership, path }) => ({
      id,
      idempotency: {
        required: contract.idempotencyRequired,
        supported: contract.idempotency,
      },
      method,
      ownership,
      path,
      requiredCiActions: metadata.requiredCiActions,
      requiredUserScopes: metadata.requiredUserScopes,
      risk: metadata.risk,
    })),
    schemaVersion: 2,
  };
}
