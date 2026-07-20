import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { parse, stringify } from "yaml";

import { composeContracts, contractOperationIds, splitContract } from "./contract-documents.mjs";
import { operationOwnership } from "./operation-ownership.mjs";

const FILES = Object.freeze({
  composed: "openapi/blobyard-composed.reference.json",
  core: "openapi/blobyard-core.openapi.yaml",
  hosted: "openapi/blobyard-hosted-extension.openapi.yaml",
  ownership: "openapi/operation-ownership.json",
  shared: "openapi/blobyard-shared.openapi.yaml",
});

async function readJson(path) {
  return JSON.parse(await readFile(path, "utf8"));
}

async function readYaml(path) {
  return parse(await readFile(path, "utf8"));
}

function paths(root) {
  return Object.fromEntries(Object.entries(FILES).map(([key, path]) => [key, resolve(root, path)]));
}

function assertSourceOwnership(core, hosted, ownership) {
  const coreIds = contractOperationIds(core).toSorted();
  const hostedIds = contractOperationIds(hosted).toSorted();
  const expectedCore = [...ownership.entries()]
    .filter(([, owner]) => owner === "core")
    .map(([id]) => id)
    .toSorted();
  const expectedHosted = [...ownership.entries()]
    .filter(([, owner]) => owner === "hosted-extension")
    .map(([id]) => id)
    .toSorted();
  if (JSON.stringify(coreIds) !== JSON.stringify(expectedCore)) {
    throw new Error("core source operations do not match ownership");
  }
  if (JSON.stringify(hostedIds) !== JSON.stringify(expectedHosted)) {
    throw new Error("hosted source operations do not match ownership");
  }
}

export async function loadComposedContract(root) {
  const sourcePaths = paths(root);
  const [core, hosted, shared, ownershipDocument] = await Promise.all([
    readYaml(sourcePaths.core),
    readYaml(sourcePaths.hosted),
    readYaml(sourcePaths.shared),
    readJson(sourcePaths.ownership),
  ]);
  const composed = composeContracts(shared, core, hosted);
  const ownership = operationOwnership(composed, ownershipDocument);
  assertSourceOwnership(core, hosted, ownership);
  return composed;
}

export async function bootstrapContractSources(root) {
  const sourcePaths = paths(root);
  const [document, ownershipDocument] = await Promise.all([
    readJson(sourcePaths.composed),
    readJson(sourcePaths.ownership),
  ]);
  const ownership = operationOwnership(document, ownershipDocument);
  const split = splitContract(document, ownership);
  const yamlOptions = { lineWidth: 100, sortMapEntries: false };
  await Promise.all([
    writeFile(sourcePaths.core, stringify(split.core, yamlOptions), "utf8"),
    writeFile(sourcePaths.hosted, stringify(split.hosted, yamlOptions), "utf8"),
    writeFile(sourcePaths.shared, stringify(split.shared, yamlOptions), "utf8"),
  ]);
}
