import { readFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const METHOD_MARKERS = Object.freeze({
  DELETE: ["delete(", ".delete("],
  GET: ["get(", ".get("],
  PATCH: ["patch(", ".patch("],
  POST: ["post(", ".post("],
  PUT: ["put(", ".put("],
});

function uniqueSorted(values, label) {
  if (!Array.isArray(values) || values.some((value) => typeof value !== "string")) {
    throw new Error(`server handler parity ${label} must be a string array`);
  }
  const sorted = values.toSorted();
  if (new Set(sorted).size !== sorted.length) {
    throw new Error(`server handler parity ${label} contains duplicates`);
  }
  return sorted;
}

function validatePartition(core, ledger) {
  if (
    ledger?.schemaVersion !== 1 ||
    typeof ledger.implemented !== "object" ||
    ledger.implemented === null ||
    Array.isArray(ledger.implemented)
  ) {
    throw new Error("server handler parity schema must be version 1");
  }
  const implemented = Object.keys(ledger.implemented).toSorted();
  const partial = uniqueSorted(ledger.partial, "partial");
  const missing = uniqueSorted(ledger.missing, "missing");
  const classified = [...implemented, ...partial, ...missing];
  if (new Set(classified).size !== classified.length) {
    throw new Error("server handler parity classifications overlap");
  }
  const expected = core.map((operation) => operation.id).toSorted();
  const absent = expected.filter((identifier) => !classified.includes(identifier));
  const stale = classified.filter((identifier) => !expected.includes(identifier));
  if (absent.length > 0)
    throw new Error(`Core operations missing handler status: ${absent.join(", ")}`);
  if (stale.length > 0)
    throw new Error(`handler status contains unknown operations: ${stale.join(", ")}`);
  return { implemented, missing, partial };
}

async function verifyImplementedRoutes(root, core, ledger, implemented) {
  const operations = new Map(core.map((operation) => [operation.id, operation]));
  for (const identifier of implemented) {
    const sourcePath = ledger.implemented[identifier];
    if (typeof sourcePath !== "string" || !sourcePath.startsWith("crates/blobyard-server/src/")) {
      throw new Error(`${identifier} has invalid handler source evidence`);
    }
    const source = await readFile(join(root, sourcePath), "utf8");
    const operation = operations.get(identifier);
    const route = `\"/v1${operation.path}\"`;
    if (!source.includes(route))
      throw new Error(`${identifier} route is absent from ${sourcePath}`);
    const markers = METHOD_MARKERS[operation.method];
    if (!markers?.some((marker) => source.includes(marker))) {
      throw new Error(`${identifier} method is absent from ${sourcePath}`);
    }
  }
}

export async function checkServerHandlerParity({
  root = ROOT,
  requireComplete = false,
  operationsDocument,
  ledgerDocument,
} = {}) {
  const [operations, ledger] = await Promise.all([
    operationsDocument ??
      readFile(join(root, "conformance/operations.json"), "utf8").then(JSON.parse),
    ledgerDocument ??
      readFile(join(root, "conformance-source/server-handler-parity.json"), "utf8").then(
        JSON.parse,
      ),
  ]);
  if (operations?.schemaVersion !== 2 || !Array.isArray(operations.operations)) {
    throw new Error("generated operation manifest must be schema version 2");
  }
  const core = operations.operations.filter((operation) => operation.ownership === "core");
  const status = validatePartition(core, ledger);
  await verifyImplementedRoutes(root, core, ledger, status.implemented);
  const pending = [...status.partial, ...status.missing].toSorted();
  if (requireComplete && pending.length > 0) {
    throw new Error(`Core server handlers are incomplete: ${pending.join(", ")}`);
  }
  return {
    core: core.length,
    implemented: status.implemented.length,
    missing: status.missing.length,
    partial: status.partial.length,
    pending,
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const result = await checkServerHandlerParity({
      requireComplete: process.argv.includes("--require-complete"),
    });
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : "handler parity failed"}\n`);
    process.exitCode = 1;
  }
}
