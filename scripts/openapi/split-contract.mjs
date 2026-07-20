#!/usr/bin/env node

import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { bootstrapContractSources, loadComposedContract } from "./contract-files.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

if (process.argv.includes("--bootstrap")) {
  await bootstrapContractSources(root);
  console.log("Bootstrapped canonical core and hosted-extension OpenAPI sources.");
} else {
  const document = await loadComposedContract(root);
  console.log(`Checked ${Object.keys(document.paths).length} composed OpenAPI paths.`);
}
