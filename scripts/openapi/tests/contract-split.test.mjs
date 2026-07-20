import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { parse } from "yaml";

import {
  composeContracts,
  contractOperationIds,
  projectContractForDeployment,
  splitContract,
} from "../contract-documents.mjs";
import { loadComposedContract } from "../contract-files.mjs";
import { operationMetadata } from "../operation-metadata.mjs";
import { operationOwnership } from "../operation-ownership.mjs";

async function sources() {
  const names = ["core", "hosted-extension", "shared"];
  const documents = await Promise.all(
    names.map(async (name) =>
      parse(await readFile(`openapi/blobyard-${name}.openapi.yaml`, "utf8")),
    ),
  );
  return { core: documents[0], hosted: documents[1], shared: documents[2] };
}

function references(value) {
  if (Array.isArray(value)) return value.flatMap(references);
  if (typeof value !== "object" || value === null) return [];
  return Object.entries(value).flatMap(([key, item]) =>
    key === "$ref" ? [item] : references(item),
  );
}

test("canonical split composes to the checked-in hosted contract", async () => {
  const [actual, expected, split] = await Promise.all([
    loadComposedContract(process.cwd()),
    readFile("openapi/blobyard-composed.reference.json", "utf8").then(JSON.parse),
    sources(),
  ]);
  assert.deepEqual(projectContractForDeployment(actual, "cloud"), expected);
  assert.deepEqual(
    projectContractForDeployment(composeContracts(split.shared, split.core, split.hosted), "cloud"),
    expected,
  );
  assert.equal(expected.paths["/bootstrap/exchange"], undefined);
  assert.notEqual(actual.paths["/bootstrap/exchange"], undefined);
  assert.equal(contractOperationIds(split.core).length, 49);
  assert.equal(contractOperationIds(split.hosted).length, 25);
  assert.deepEqual(split.shared.paths, {});
});

test("split documents reference only the canonical shared components", async () => {
  const split = await sources();
  for (const document of [split.core, split.hosted]) {
    const found = references(document);
    assert.ok(found.length > 0);
    assert.ok(
      found.every((reference) =>
        reference.startsWith("./blobyard-shared.openapi.yaml#/components/"),
      ),
    );
  }
});

test("GitHub OIDC uses one bearer-token request contract for hosted and self-hosted servers", async () => {
  const document = await loadComposedContract(process.cwd());
  const exchange = document.paths["/ci/github/oidc/exchange"].post;
  assert.deepEqual(exchange.security, [{ githubOidc: [] }]);
  assert.equal(
    exchange.requestBody.content["application/json"].schema.$ref,
    "#/components/schemas/ExchangeGitHubOidcRequest",
  );
  const request = document.components.schemas.ExchangeGitHubOidcRequest;
  assert.deepEqual(request.required, ["actions", "project"]);
  assert.deepEqual(Object.keys(request.properties).toSorted(), ["actions", "project", "workspace"]);
  assert.equal(request.properties.actions.uniqueItems, true);
  assert.deepEqual(document.components.schemas.CiAction.enum, [
    "upload",
    "share",
    "download",
    "yard:manage",
  ]);
});

test("operation metadata covers every contract operation and rejects drift", async () => {
  const [document, source, generated] = await Promise.all([
    loadComposedContract(process.cwd()),
    readFile("openapi/operation-metadata.json", "utf8").then(JSON.parse),
    readFile("conformance/operations.json", "utf8").then(JSON.parse),
  ]);
  const identifiers = contractOperationIds(document).toSorted();
  const metadata = operationMetadata(identifiers, source);
  assert.equal(metadata.size, 74);
  assert.deepEqual(metadata.get("requestUpload"), {
    requiredCiActions: ["upload"],
    requiredUserScopes: ["object:write"],
    risk: "write",
  });
  assert.equal(generated.schemaVersion, 2);
  assert.deepEqual(
    generated.operations.find((operation) => operation.id === "requestUpload"),
    {
      id: "requestUpload",
      idempotency: { required: true, supported: true },
      method: "POST",
      ownership: "core",
      path: "/uploads/request",
      requiredCiActions: ["upload"],
      requiredUserScopes: ["object:write"],
      risk: "write",
    },
  );

  const missing = structuredClone(source);
  delete missing.operations.health;
  assert.throws(() => operationMetadata(identifiers, missing), /missing metadata/u);
  const stale = structuredClone(source);
  stale.operations.unknown = stale.operations.health;
  assert.throws(() => operationMetadata(identifiers, stale), /unknown operations/u);
  const invalid = structuredClone(source);
  invalid.operations.health.requiredUserScopes = ["unknown:scope"];
  assert.throws(() => operationMetadata(identifiers, invalid), /invalid user scopes/u);
});

test("split and composition fail closed for internal, duplicate, missing, and unordered paths", async () => {
  const [document, ownershipDocument, split] = await Promise.all([
    loadComposedContract(process.cwd()),
    readFile("openapi/operation-ownership.json", "utf8").then(JSON.parse),
    sources(),
  ]);
  const ownership = operationOwnership(document, ownershipDocument);
  ownership.set("health", "internal");
  assert.throws(() => splitContract(document, ownership), /internal operations/u);

  const duplicateHosted = structuredClone(split.hosted);
  duplicateHosted.paths["/health"] = structuredClone(split.core.paths["/health"]);
  assert.throws(
    () => composeContracts(split.shared, split.core, duplicateHosted),
    /duplicate get operation/u,
  );

  const missingCore = structuredClone(split.core);
  delete missingCore.paths["/health"];
  assert.throws(
    () => composeContracts(split.shared, missingCore, split.hosted),
    /contract path \/health is missing/u,
  );

  const unordered = structuredClone(split.shared);
  unordered["x-blobyard-path-order"].pop();
  assert.throws(
    () => composeContracts(unordered, split.core, split.hosted),
    /path order is incomplete/u,
  );
});
