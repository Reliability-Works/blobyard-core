import assert from "node:assert/strict";
import { cpSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { parse, stringify } from "yaml";

import { generateConformance } from "../generate-conformance.mjs";

const ROOT = resolve(import.meta.dirname, "../../..");

function fixtureRepository() {
  const root = mkdtempSync(join(tmpdir(), "blobyard-conformance-"));
  mkdirSync(join(root, "conformance"), { recursive: true });
  cpSync(join(ROOT, "conformance-source/fixtures"), join(root, "conformance-source/fixtures"), {
    recursive: true,
  });
  cpSync(join(ROOT, "openapi"), join(root, "openapi"), { recursive: true });
  writeFileSync(join(root, "Cargo.toml"), '[workspace.package]\nversion = "1.2.3"\n');
  writeFileSync(join(root, "conformance/operations.json"), '{"operations":[]}\n');
  return root;
}

test("generates and verifies a deterministic checksummed bundle", async () => {
  const root = fixtureRepository();
  try {
    assert.deepEqual(await generateConformance({ root }), {
      files: 11,
      root: "conformance",
    });
    assert.equal((await generateConformance({ check: true, root })).files, 11);
    const manifest = JSON.parse(readFileSync(join(root, "conformance/manifest.json")));
    assert.equal(manifest.coreVersion, "1.2.3");
    assert.equal(manifest.members.length, 11);
    writeFileSync(join(root, "conformance/behavior/cache.json"), "stale\n");
    await assert.rejects(generateConformance({ check: true, root }), /is stale/u);
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
});

test("rejects duplicate identifiers and sensitive fixture fields", async () => {
  const root = fixtureRepository();
  try {
    const path = join(root, "conformance-source/fixtures/cache.yaml");
    const fixture = parse(readFileSync(path, "utf8"));
    fixture.cases.push(fixture.cases[0]);
    writeFileSync(path, stringify(fixture));
    await assert.rejects(generateConformance({ root }), /duplicate fixture identifier/u);
    fixture.cases.pop();
    fixture.cases[0].accessToken = "forbidden";
    writeFileSync(path, stringify(fixture));
    await assert.rejects(generateConformance({ root }), /forbidden sensitive field/u);
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
});
