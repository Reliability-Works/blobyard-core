#!/usr/bin/env node

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { format, resolveConfig } from "prettier";

import { operationContract, pascal, validateClassifications } from "./schema-tools.mjs";
import { projectContractForDeployment } from "./contract-documents.mjs";
import { loadComposedContract } from "./contract-files.mjs";
import { operationMetadata, operationManifest } from "./operation-metadata.mjs";
import { operationOwnership } from "./operation-ownership.mjs";
import { sdkDeclarations } from "./sdk-declarations.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const documentArgument = process.argv.indexOf("--document");
const documentPath =
  documentArgument === -1
    ? resolve(root, "openapi/blobyard-composed.reference.json")
    : resolve(process.cwd(), process.argv[documentArgument + 1] ?? "");
const check = process.argv.includes("--check");
const methods = new Set(["delete", "get", "patch", "post", "put"]);
const ownershipPath = resolve(root, "openapi/operation-ownership.json");
const metadataPath = resolve(root, "openapi/operation-metadata.json");
const deployments = new Set(["cloud", "self-hosted"]);

const outputs = {
  api: resolve(root, "crates/blobyard-api-client/tests/generated/openapi_operations.rs"),
  cli: resolve(root, "crates/blobyard-cli/tests/generated/openapi_operations.rs"),
  composed: resolve(root, "openapi/blobyard-composed.reference.json"),
  docs: resolve(root, "docs/api-surfaces.generated.md"),
  mcp: resolve(root, "crates/blobyard-mcp/src/openapi_operations.generated.rs"),
  ownership: resolve(root, "conformance/operations.json"),
  sdkDeclarations: resolve(root, "sdk/typescript/src/operations.generated.d.mts"),
  sdkOperations: resolve(root, "sdk/typescript/src/operations.generated.mjs"),
};

function fail(message) {
  throw new Error(`OpenAPI contract: ${message}`);
}

function operationIdentifiers(document) {
  return new Set(
    Object.values(document.paths ?? {}).flatMap((item) =>
      Object.values(item).flatMap((operation) =>
        typeof operation?.operationId === "string" ? [operation.operationId] : [],
      ),
    ),
  );
}

function ownershipForDocument(document, manifest) {
  if (documentArgument === -1) return manifest;
  const identifiers = operationIdentifiers(document);
  return {
    ...manifest,
    core: manifest.core.filter((identifier) => identifiers.has(identifier)),
    hostedExtension: manifest.hostedExtension.filter((identifier) => identifiers.has(identifier)),
    internal: manifest.internal.filter((identifier) => identifiers.has(identifier)),
  };
}

function metadataForDocument(document, manifest) {
  if (documentArgument === -1) return manifest;
  const identifiers = operationIdentifiers(document);
  return {
    ...manifest,
    operations: Object.fromEntries(
      Object.entries(manifest.operations).filter(([identifier]) => identifiers.has(identifier)),
    ),
  };
}

function surface(operation, name) {
  const value = operation[`x-blobyard-${name}`];
  if (value === undefined) fail(`${operation.operationId} is missing ${name} surface metadata`);
  if (name === "sdk") {
    if (value !== true) fail(`${operation.operationId} must be available through the SDK`);
    return value;
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    fail(`${operation.operationId} has invalid ${name} surface metadata`);
  }
  const adapter = name === "cli" ? value.command : value.tool;
  const hasAdapter =
    name === "cli" ? Array.isArray(adapter) && adapter.length > 0 : typeof adapter === "string";
  const hasExclusion = typeof value.excluded === "string" && value.excluded.length > 0;
  if (hasAdapter === hasExclusion) {
    fail(`${operation.operationId} must declare exactly one ${name} adapter or exclusion`);
  }
  if (
    name === "cli" &&
    hasAdapter &&
    !adapter.every((part) => typeof part === "string" && part.length > 0)
  ) {
    fail(`${operation.operationId} has an invalid CLI command path`);
  }
  if (name === "mcp" && hasAdapter && !adapter.startsWith("blobyard_")) {
    fail(`${operation.operationId} has an invalid MCP tool name`);
  }
  return value;
}

function operations(document, ownershipDocument) {
  if (document.openapi !== "3.1.0") fail("openapi must be 3.1.0");
  if (
    !Array.isArray(document.servers) ||
    document.servers[0]?.url !== "https://api.blobyard.com/v1"
  ) {
    fail("the canonical production server must be https://api.blobyard.com/v1");
  }
  try {
    validateClassifications(document);
  } catch (error) {
    fail(error instanceof Error ? error.message : "invalid UI-state classification");
  }
  const ownership = operationOwnership(document, ownershipDocument);
  const found = [];
  const identifiers = new Set();
  for (const [path, item] of Object.entries(document.paths ?? {})) {
    if (!path.startsWith("/") || path.startsWith("/v1/")) fail(`invalid relative path ${path}`);
    for (const [method, operation] of Object.entries(item)) {
      if (!methods.has(method)) continue;
      if (typeof operation?.operationId !== "string" || operation.operationId.length === 0) {
        fail(`${method.toUpperCase()} ${path} is missing operationId`);
      }
      if (identifiers.has(operation.operationId))
        fail(`duplicate operationId ${operation.operationId}`);
      identifiers.add(operation.operationId);
      const cli = surface(operation, "cli");
      const mcp = surface(operation, "mcp");
      surface(operation, "sdk");
      const owner = ownership.get(operation.operationId);
      const configuredDeployments = operation["x-blobyard-deployments"];
      const operationDeployments =
        configuredDeployments ?? (owner === "core" ? ["cloud", "self-hosted"] : ["cloud"]);
      if (
        !Array.isArray(operationDeployments) ||
        operationDeployments.length === 0 ||
        operationDeployments.some((deployment) => !deployments.has(deployment)) ||
        new Set(operationDeployments).size !== operationDeployments.length ||
        (owner === "hosted-extension" && operationDeployments.includes("self-hosted"))
      ) {
        fail(`${operation.operationId} has invalid deployment availability`);
      }
      const hasRequestParameters = operation.parameters?.some((item) =>
        ["path", "query"].includes(item.in),
      );
      if (method !== "get" && operation.requestBody === undefined && !hasRequestParameters) {
        fail(`${operation.operationId} is missing its input schema`);
      }
      let contract;
      try {
        contract = operationContract(document, path, operation);
      } catch (error) {
        fail(
          error instanceof Error ? error.message : `${operation.operationId} has invalid schemas`,
        );
      }
      found.push({
        cli,
        contract: { ...contract, inputName: `${pascal(operation.operationId)}Input` },
        id: operation.operationId,
        mcp,
        method: method.toUpperCase(),
        ownership: owner,
        path,
        public: Array.isArray(operation.security) && operation.security.length === 0,
        deployments: operationDeployments,
      });
    }
  }
  if (found.length === 0) fail("no operations were found");
  return found.sort((left, right) => left.id.localeCompare(right.id));
}

function quoted(value) {
  return JSON.stringify(value);
}

function sdkOperations(items) {
  const entries = items
    .map(
      (item) =>
        `  ${quoted(item.id)}: Object.freeze({ deployments: Object.freeze(${quoted(item.deployments)}), idempotency: ${item.contract.idempotency}, idempotencyRequired: ${item.contract.idempotencyRequired}, method: ${quoted(item.method)}, ownership: ${quoted(item.ownership)}, path: ${quoted(item.path)}, public: ${item.public}, requiredCiActions: Object.freeze(${quoted(item.metadata.requiredCiActions)}), requiredUserScopes: Object.freeze(${quoted(item.metadata.requiredUserScopes)}), risk: ${quoted(item.metadata.risk)}, successStatus: ${item.contract.successStatus} })`,
    )
    .join(",\n");
  const bindings = items
    .map((item) => `    ${quoted(item.id)}: (options = {}) => request(${quoted(item.id)}, options)`)
    .join(",\n");
  return `// Generated by scripts/openapi/generate.mjs. Do not edit.\n\nexport const operations = Object.freeze({\n${entries}\n});\n\nexport function bindOperations(request) {\n  return Object.freeze({\n${bindings}\n  });\n}\n`;
}

function rustString(value) {
  return JSON.stringify(value);
}

function cliManifest(items) {
  const mapped = items.filter((item) => Array.isArray(item.cli.command));
  const entries = mapped
    .map((item) => {
      const command = item.cli.command.map(rustString).join(", ");
      return `    (${rustString(item.id)}, ${rustString(`/v1${item.path}`)}, ${rustString(item.method)}, ${item.contract.idempotency}, &[${command}]),`;
    })
    .join("\n");
  return `// Generated by scripts/openapi/generate.mjs. Do not edit.\n\n#[rustfmt::skip]\nconst OPENAPI_CLI_OPERATIONS: &[(&str, &str, &str, bool, &[&str])] = &[\n${entries}\n];\n`;
}

function apiManifest(items) {
  const entries = items
    .map(
      (item) =>
        `    (${rustString(item.id)}, ${rustString(`/v1${item.path}`)}, ${rustString(item.method)}, ${item.contract.idempotency}),`,
    )
    .join("\n");
  return `// Generated by scripts/openapi/generate.mjs. Do not edit.\n\n#[rustfmt::skip]\nconst OPENAPI_API_OPERATIONS: &[(&str, &str, &str, bool)] = &[\n${entries}\n];\n`;
}

function mcpManifest(items) {
  const mapped = items.filter((item) => typeof item.mcp.tool === "string");
  const entries = mapped
    .map(
      (item) =>
        `    (${rustString(item.id)}, ${rustString(`/v1${item.path}`)}, ${rustString(item.method)}, ${rustString(item.mcp.tool)}),`,
    )
    .join("\n");
  return `// Generated by scripts/openapi/generate.mjs. Do not edit.\n\n#[rustfmt::skip]\nconst OPENAPI_MCP_OPERATIONS: &[(&str, &str, &str, &str)] = &[\n${entries}\n];\n`;
}

function surfaceLabel(value, name) {
  if (typeof value.excluded === "string") return `Excluded: ${value.excluded}`;
  return name === "CLI" ? `\`${value.command.join(" ")}\`` : `\`${value.tool}\``;
}

function docsManifest(items) {
  const rows = items
    .map(
      (item) =>
        `| \`${item.id}\` | \`${item.method} ${item.path}\` | \`${item.ownership}\` | ${authorizationLabel(item)} | \`${item.metadata.risk}\` | ${surfaceLabel(item.cli, "CLI")} | ${surfaceLabel(item.mcp, "MCP")} |`,
    )
    .join("\n");
  return `<!-- Generated by scripts/openapi/generate.mjs. Do not edit. -->\n\n# API surface parity\n\nEvery public operation is available through the TypeScript SDK. Ownership identifies the self-hosted core contract or a Blob Yard Cloud extension. Authorization records the required user scopes and alternative CI actions. Risk classifies read, write, sensitive, and destructive operations. The CLI and MCP columns name the explicit presentation adapter, or explain why a direct adapter would be unsafe or meaningless.\n\nOnboarding progress is explicitly classified as derived browser UI state. It is not a stable API resource, so it is excluded from the SDK, CLI, and MCP surfaces.\n\n| Operation | HTTP | Ownership | Authorization | Risk | CLI | MCP |\n| --- | --- | --- | --- | --- | --- | --- |\n${rows}\n`;
}

function authorizationLabel(item) {
  const user = item.metadata.requiredUserScopes.map((scope) => `user:\`${scope}\``);
  const ci = item.metadata.requiredCiActions.map((action) => `ci:\`${action}\``);
  const alternatives = [...user, ...ci];
  return alternatives.length === 0 ? "Special or public authority" : alternatives.join("<br>");
}

async function update(path, expected) {
  const prettierConfig = (await resolveConfig(path)) ?? {};
  const formatted = path.endsWith(".rs")
    ? expected
    : await format(expected, { ...prettierConfig, filepath: path });
  let current = "";
  try {
    current = await readFile(path, "utf8");
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  if (current === formatted) return;
  if (check) fail(`${path.slice(root.length + 1)} is stale; run pnpm openapi:generate`);
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, formatted, "utf8");
}

const document =
  documentArgument === -1
    ? await loadComposedContract(root)
    : JSON.parse(await readFile(documentPath, "utf8"));
const [ownershipSource, metadataSource] = await Promise.all([
  readFile(ownershipPath, "utf8").then(JSON.parse),
  readFile(metadataPath, "utf8").then(JSON.parse),
]);
const ownershipDocument = ownershipForDocument(document, ownershipSource);
const rawItems = operations(document, ownershipDocument);
const metadata = operationMetadata(
  rawItems.map((item) => item.id),
  metadataForDocument(document, metadataSource),
);
const items = rawItems.map((item) => ({ ...item, metadata: metadata.get(item.id) }));
const hostedDocument = projectContractForDeployment(document, "cloud");
await Promise.all([
  update(outputs.api, apiManifest(items)),
  update(outputs.cli, cliManifest(items)),
  update(outputs.composed, `${JSON.stringify(hostedDocument, null, 2)}\n`),
  update(outputs.docs, docsManifest(items)),
  update(outputs.mcp, mcpManifest(items)),
  update(outputs.ownership, `${JSON.stringify(operationManifest(items), null, 2)}\n`),
  update(outputs.sdkDeclarations, sdkDeclarations(document, items)),
  update(outputs.sdkOperations, sdkOperations(items)),
]);
console.log(`${check ? "Checked" : "Generated"} ${items.length} OpenAPI operations.`);
