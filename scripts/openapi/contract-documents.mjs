const METHODS = new Set(["delete", "get", "patch", "post", "put"]);
const SHARED_REFERENCE = "./blobyard-shared.openapi.yaml#/components/";
const DEPLOYMENTS = new Set(["cloud", "self-hosted"]);

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function rewriteReferences(value, rewrite) {
  if (Array.isArray(value)) return value.map((item) => rewriteReferences(item, rewrite));
  if (!isRecord(value)) return value;
  return Object.fromEntries(
    Object.entries(value).map(([key, item]) => [
      key,
      key === "$ref" && typeof item === "string" ? rewrite(item) : rewriteReferences(item, rewrite),
    ]),
  );
}

function externalize(value) {
  return rewriteReferences(value, (reference) =>
    reference.startsWith("#/components/")
      ? `${SHARED_REFERENCE}${reference.slice("#/components/".length)}`
      : reference,
  );
}

function internalize(value) {
  return rewriteReferences(value, (reference) =>
    reference.startsWith(SHARED_REFERENCE)
      ? `#/components/${reference.slice(SHARED_REFERENCE.length)}`
      : reference,
  );
}

function securityAliases(document) {
  const entries = Object.keys(document.components?.securitySchemes ?? {}).map((name) => [
    name,
    { $ref: `${SHARED_REFERENCE}securitySchemes/${name}` },
  ]);
  return entries.length === 0 ? undefined : { securitySchemes: Object.fromEntries(entries) };
}

function splitPathItem(item, ownership) {
  const base = Object.fromEntries(Object.entries(item).filter(([key]) => !METHODS.has(key)));
  const split = { core: { ...base }, "hosted-extension": { ...base } };
  for (const [method, operation] of Object.entries(item)) {
    if (!METHODS.has(method)) continue;
    const owner = ownership.get(operation?.operationId);
    if (owner === "internal") throw new Error("internal operations cannot enter public contracts");
    if (owner !== "core" && owner !== "hosted-extension") {
      throw new Error(`operation ${operation?.operationId ?? "unknown"} has no contract owner`);
    }
    split[owner][method] = operation;
  }
  return split;
}

function splitPaths(document, ownership) {
  const core = {};
  const hosted = {};
  for (const [path, item] of Object.entries(document.paths ?? {})) {
    const split = splitPathItem(item, ownership);
    if (Object.keys(split.core).some((key) => METHODS.has(key))) core[path] = split.core;
    if (Object.keys(split["hosted-extension"]).some((key) => METHODS.has(key))) {
      hosted[path] = split["hosted-extension"];
    }
  }
  return { core, hosted };
}

function contractDocument(document, title, kind, paths) {
  return externalize({
    openapi: document.openapi,
    info: { ...document.info, title },
    servers: document.servers,
    security: document.security,
    paths,
    components: securityAliases(document),
    "x-blobyard-contract": kind,
  });
}

export function splitContract(document, ownership) {
  const paths = splitPaths(document, ownership);
  const shared = structuredClone(document);
  shared.paths = {};
  shared["x-blobyard-path-order"] = Object.keys(document.paths ?? {});
  return {
    core: contractDocument(document, "Blob Yard Core API", "core", paths.core),
    hosted: contractDocument(
      document,
      "Blob Yard Cloud Extensions API",
      "hosted-extension",
      paths.hosted,
    ),
    shared,
  };
}

function mergeMetadata(target, source) {
  for (const [key, value] of Object.entries(source)) {
    if (METHODS.has(key)) continue;
    if (target[key] !== undefined && JSON.stringify(target[key]) !== JSON.stringify(value)) {
      throw new Error(`conflicting path metadata ${key}`);
    }
    target[key] = value;
  }
}

function mergePath(target, source) {
  if (source === undefined) return;
  const internal = internalize(source);
  mergeMetadata(target, internal);
  for (const [method, operation] of Object.entries(internal)) {
    if (!METHODS.has(method)) continue;
    if (target[method] !== undefined) throw new Error(`duplicate ${method} operation`);
    target[method] = operation;
  }
}

export function composeContracts(sharedSource, coreSource, hostedSource) {
  const shared = structuredClone(sharedSource);
  const pathOrder = shared["x-blobyard-path-order"];
  if (!Array.isArray(pathOrder)) throw new Error("shared contract is missing path order");
  delete shared["x-blobyard-path-order"];
  shared.paths = {};
  const knownPaths = new Set([
    ...Object.keys(coreSource.paths ?? {}),
    ...Object.keys(hostedSource.paths ?? {}),
  ]);
  for (const path of pathOrder) {
    if (!knownPaths.delete(path)) throw new Error(`contract path ${path} is missing`);
    const item = {};
    mergePath(item, coreSource.paths?.[path]);
    mergePath(item, hostedSource.paths?.[path]);
    shared.paths[path] = item;
  }
  if (knownPaths.size > 0) throw new Error(`contract path order is incomplete: ${[...knownPaths]}`);
  return shared;
}

function operationDeployments(operation) {
  const configured = operation?.["x-blobyard-deployments"];
  if (configured === undefined) return DEPLOYMENTS;
  if (
    !Array.isArray(configured) ||
    configured.length === 0 ||
    configured.some((deployment) => !DEPLOYMENTS.has(deployment))
  ) {
    throw new Error(`operation ${operation?.operationId ?? "unknown"} has invalid deployments`);
  }
  return new Set(configured);
}

export function projectContractForDeployment(document, deployment) {
  if (!DEPLOYMENTS.has(deployment)) throw new Error(`unknown deployment ${deployment}`);
  const projected = structuredClone(document);
  projected.paths = Object.fromEntries(
    Object.entries(projected.paths ?? {}).flatMap(([path, item]) => {
      const filtered = Object.fromEntries(
        Object.entries(item).filter(
          ([method, operation]) =>
            !METHODS.has(method) || operationDeployments(operation).has(deployment),
        ),
      );
      return Object.keys(filtered).some((key) => METHODS.has(key)) ? [[path, filtered]] : [];
    }),
  );
  return projected;
}

export function contractOperationIds(document) {
  return Object.values(document.paths ?? {}).flatMap((item) =>
    Object.entries(item)
      .filter(([method]) => METHODS.has(method))
      .map(([, operation]) => operation.operationId),
  );
}
