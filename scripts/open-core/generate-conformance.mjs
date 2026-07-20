import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { format } from "prettier";
import { parse } from "yaml";

const SCRIPT_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const FIXTURES = [
  ["authorization.yaml", "authorization/vectors.json", "vectors"],
  ["cache.yaml", "behavior/cache.json", "cases"],
  ["cleanup.yaml", "behavior/cleanup.json", "cases"],
  ["failures.yaml", "behavior/failures.json", "cases"],
  ["grants.yaml", "behavior/grants.json", "cases"],
  ["ranges.yaml", "behavior/ranges.json", "cases"],
  ["retention.yaml", "behavior/retention.json", "cases"],
];
const SENSITIVE_KEY = /(?:secret|token|password|credential|privateKey)/iu;

function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.entries(value)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, item]) => [key, canonical(item)]),
  );
}

function jsonReferences(value) {
  if (Array.isArray(value)) return value.map(jsonReferences);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.entries(value).map(([key, item]) => [
      key,
      key === "$ref" && typeof item === "string"
        ? item.replace("blobyard-shared.openapi.yaml", "blobyard-shared.openapi.json")
        : jsonReferences(item),
    ]),
  );
}

function validateSafeValue(value, path = "fixture") {
  if (Array.isArray(value)) {
    value.forEach((item, index) => validateSafeValue(item, `${path}[${index}]`));
    return;
  }
  if (!value || typeof value !== "object") return;
  for (const [key, item] of Object.entries(value)) {
    if (SENSITIVE_KEY.test(key)) throw new Error(`${path}.${key} is a forbidden sensitive field.`);
    validateSafeValue(item, `${path}.${key}`);
  }
}

function fixtureDocument(path, collection) {
  const document = parse(readFileSync(path, "utf8"));
  const items = document?.[collection];
  if (document?.schemaVersion !== 1 || !Array.isArray(items) || items.length === 0) {
    throw new Error(`${path} has an invalid fixture schema.`);
  }
  const identifiers = new Set();
  for (const item of items) {
    if (typeof item?.id !== "string" || !item.expected || identifiers.has(item.id)) {
      throw new Error(`${path} contains an invalid or duplicate fixture identifier.`);
    }
    identifiers.add(item.id);
  }
  validateSafeValue(document);
  return canonical({
    ...document,
    [collection]: [...items].sort((a, b) => a.id.localeCompare(b.id)),
  });
}

async function rendered(document) {
  return format(JSON.stringify(document), { parser: "json", printWidth: 100 });
}

function digest(content) {
  return createHash("sha256").update(content).digest("hex");
}

function workspaceVersion(root) {
  const source = readFileSync(join(root, "Cargo.toml"), "utf8");
  const match = source.match(/\[workspace\.package\][\s\S]*?\nversion = "([^"]+)"/u);
  if (!match) throw new Error("Workspace version is missing.");
  return match[1];
}

function writeOrCheck(path, content, check) {
  if (check) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== content) {
      throw new Error(`${path} is stale. Run the conformance generator.`);
    }
    return;
  }
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content);
}

async function buildFiles(root) {
  const fixtureRoot = join(root, "conformance-source/fixtures");
  const generatedRoot = join(root, "conformance");
  const files = new Map();
  for (const [source, target, collection] of FIXTURES) {
    files.set(target, await rendered(fixtureDocument(join(fixtureRoot, source), collection)));
  }
  const operations = readFileSync(join(generatedRoot, "operations.json"), "utf8");
  files.set("operations.json", operations);
  for (const [name, output] of [
    ["core", "blobyard-core.openapi.json"],
    ["hosted-extension", "blobyard-hosted-extension.openapi.json"],
    ["shared", "blobyard-shared.openapi.json"],
  ]) {
    const source = parse(readFileSync(join(root, `openapi/blobyard-${name}.openapi.yaml`), "utf8"));
    files.set(output, await rendered(canonical(jsonReferences(source))));
  }
  return { files, generatedRoot };
}

export async function generateConformance({ root = SCRIPT_ROOT, check = false } = {}) {
  const { files, generatedRoot } = await buildFiles(root);
  for (const [path, content] of files) {
    writeOrCheck(join(generatedRoot, path), content, check);
  }
  const members = [...files]
    .map(([path, content]) => ({ path, sha256: digest(content), size: Buffer.byteLength(content) }))
    .sort((left, right) => left.path.localeCompare(right.path));
  const manifest = await rendered({
    coreVersion: workspaceVersion(root),
    members,
    schemaVersion: 1,
    sourceRevision: "unreleased",
  });
  writeOrCheck(join(generatedRoot, "manifest.json"), manifest, check);
  const sums = [...members, { path: "manifest.json", sha256: digest(manifest) }]
    .map((member) => `${member.sha256}  ${member.path}`)
    .join("\n");
  writeOrCheck(join(generatedRoot, "SHA256SUMS"), `${sums}\n`, check);
  return { files: members.length, root: relative(root, generatedRoot) };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const result = await generateConformance({ check: process.argv.includes("--check") });
    process.stdout.write(`Conformance bundle passed: ${result.files} members.\n`);
  } catch (error) {
    process.stderr.write(
      `${error instanceof Error ? error.message : "Conformance generation failed."}\n`,
    );
    process.exitCode = 1;
  }
}
