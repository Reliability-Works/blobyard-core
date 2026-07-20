import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  compareVersions,
  isReleaseImpactingPath,
  nextPatchVersion,
  parseVersion,
  replaceSdkVersion,
  replaceWorkspaceVersion,
  sdkVersion,
  workspaceVersion,
} from "../cli-version.mjs";

const SCRIPT_PATH = resolve(dirname(fileURLToPath(import.meta.url)), "../cli-version.mjs");

function run(command, args, cwd) {
  return execFileSync(command, args, { cwd, encoding: "utf8" });
}

function createReleaseRepository() {
  const repo = mkdtempSync(join(tmpdir(), "blobyard-cli-version-"));
  mkdirSync(join(repo, "crates/blobyard-cli/src"), { recursive: true });
  mkdirSync(join(repo, "sdk/typescript"), { recursive: true });
  writeFileSync(
    join(repo, "Cargo.toml"),
    `[workspace]\nmembers = ["crates/blobyard-cli"]\nresolver = "2"\n\n[workspace.package]\nversion = "0.1.9"\nedition = "2024"\n`,
  );
  writeFileSync(
    join(repo, "crates/blobyard-cli/Cargo.toml"),
    `[package]\nname = "blobyard-cli"\nversion.workspace = true\nedition.workspace = true\n`,
  );
  writeFileSync(join(repo, "crates/blobyard-cli/src/lib.rs"), "pub fn value() -> u8 { 1 }\n");
  writeFileSync(
    join(repo, "sdk/typescript/package.json"),
    `${JSON.stringify({ name: "@blobyard/sdk", version: "0.1.9", private: true }, null, 2)}\n`,
  );
  writeFileSync(
    join(repo, "crates/blobyard-cli/src/removed.rs"),
    "pub fn old_value() -> u8 { 1 }\n",
  );
  run("git", ["init", "--quiet"], repo);
  run("git", ["config", "user.email", "test@example.invalid"], repo);
  run("git", ["config", "user.name", "Blob Yard test"], repo);
  run("cargo", ["generate-lockfile", "--offline"], repo);
  run("git", ["add", "."], repo);
  run("git", ["commit", "--quiet", "-m", "fixture"], repo);
  run("git", ["tag", "v0.1.9"], repo);
  return repo;
}

function createFreshRepository() {
  const repo = mkdtempSync(join(tmpdir(), "blobyard-core-version-"));
  mkdirSync(join(repo, "crates/blobyard-cli/src"), { recursive: true });
  mkdirSync(join(repo, "release"), { recursive: true });
  mkdirSync(join(repo, "sdk/typescript"), { recursive: true });
  writeFileSync(
    join(repo, "Cargo.toml"),
    `[workspace]\nmembers = ["crates/blobyard-cli"]\nresolver = "2"\n\n[workspace.package]\nversion = "0.1.12"\nedition = "2024"\n`,
  );
  writeFileSync(
    join(repo, "crates/blobyard-cli/Cargo.toml"),
    `[package]\nname = "blobyard-cli"\nversion.workspace = true\nedition.workspace = true\n`,
  );
  writeFileSync(join(repo, "crates/blobyard-cli/src/lib.rs"), "pub fn value() -> u8 { 1 }\n");
  writeFileSync(
    join(repo, "sdk/typescript/package.json"),
    `${JSON.stringify({ name: "@blobyard/sdk", version: "0.1.12", private: true }, null, 2)}\n`,
  );
  writeFileSync(
    join(repo, "release/baseline.json"),
    JSON.stringify({
      previousRepository: "Reliability-Works/blobyard",
      previousVersion: "0.1.11",
      schemaVersion: 1,
      sourceRevision: "a".repeat(40),
    }),
  );
  run("git", ["init", "--quiet"], repo);
  return repo;
}

test("classifies shipped CLI and distribution paths", () => {
  assert.equal(isReleaseImpactingPath("crates/blobyard-cli/src/main.rs"), true);
  assert.equal(isReleaseImpactingPath("crates/blobyard-api-client/src/client.rs"), true);
  assert.equal(isReleaseImpactingPath("crates/blobyard-cli/Cargo.toml"), true);
  assert.equal(isReleaseImpactingPath("crates/blobyard-mcp/Cargo.toml"), true);
  assert.equal(isReleaseImpactingPath("Cargo.lock"), true);
  assert.equal(isReleaseImpactingPath("scripts/install.sh"), true);
  assert.equal(isReleaseImpactingPath("release/package-artifact.sh"), true);
  assert.equal(isReleaseImpactingPath(".github/actions/upload/action.yml"), true);
  assert.equal(isReleaseImpactingPath("crates/blobyard-cli/tests/commands.rs"), false);
  assert.equal(isReleaseImpactingPath("release/tests/manifest-self-test.sh"), false);
  assert.equal(isReleaseImpactingPath("docs/release.md"), false);
});

test("parses, compares, and increments semantic versions", () => {
  assert.deepEqual(parseVersion("1.2.3"), [1, 2, 3]);
  assert.equal(compareVersions("1.2.3", "1.2.3"), 0);
  assert.equal(compareVersions("1.3.0", "1.2.9"), 1);
  assert.equal(compareVersions("1.2.2", "1.2.3"), -1);
  assert.equal(nextPatchVersion("1.2.9"), "1.2.10");
  assert.throws(() => parseVersion("latest"), /Expected a semantic version/);
});

test("accepts the fresh Core version ahead of the final private-repository baseline", () => {
  const repo = createFreshRepository();
  try {
    const checked = run("node", [SCRIPT_PATH, "check"], repo);

    assert.match(checked, /passed at 0\.1\.12; latest product release baseline v0\.1\.11/u);
  } finally {
    rmSync(repo, { recursive: true, force: true });
  }
});

test("reads and replaces only the workspace package version", () => {
  const source = `[workspace]\nmembers = []\n\n[workspace.package]\nversion = "0.1.9"\n\n[dependencies]\nfixture = "2.0.0"\n`;
  const updated = replaceWorkspaceVersion(source, "0.1.10");

  assert.equal(workspaceVersion(source), "0.1.9");
  assert.equal(workspaceVersion(updated), "0.1.10");
  assert.match(updated, /fixture = "2\.0\.0"/);
  assert.throws(() => workspaceVersion("[workspace]\n"), /does not define/);
  assert.throws(() => replaceWorkspaceVersion("[workspace]\n", "0.1.10"), /does not define/);
});

test("reads and replaces the TypeScript SDK version", () => {
  const source = `${JSON.stringify({ name: "@blobyard/sdk", version: "1.2.3", private: true }, null, 2)}\n`;
  const updated = replaceSdkVersion(source, "1.2.4");

  assert.equal(sdkVersion(source), "1.2.3");
  assert.equal(sdkVersion(updated), "1.2.4");
  assert.equal(JSON.parse(updated).private, true);
  assert.throws(() => sdkVersion('{"name":"fixture"}\n'), /does not define a version/u);
});

test("prepares one patch version for staged shipped code", () => {
  const repo = createReleaseRepository();
  try {
    writeFileSync(join(repo, "crates/blobyard-cli/src/lib.rs"), "pub fn value() -> u8 { 2 }\n");
    run("git", ["add", "crates/blobyard-cli/src/lib.rs"], repo);

    const first = run("node", [SCRIPT_PATH, "prepare"], repo);
    const second = run("node", [SCRIPT_PATH, "prepare"], repo);
    const checked = run("node", [SCRIPT_PATH, "check"], repo);
    const staged = run("git", ["diff", "--cached", "--name-only"], repo);

    assert.match(first, /Prepared CLI release 0\.1\.10/);
    assert.match(second, /already ahead/);
    assert.match(checked, /passed at 0\.1\.10/);
    assert.equal(workspaceVersion(readFileSync(join(repo, "Cargo.toml"), "utf8")), "0.1.10");
    assert.equal(
      sdkVersion(readFileSync(join(repo, "sdk/typescript/package.json"), "utf8")),
      "0.1.10",
    );
    assert.match(readFileSync(join(repo, "Cargo.lock"), "utf8"), /version = "0\.1\.10"/);
    assert.match(staged, /Cargo\.toml/);
    assert.match(staged, /Cargo\.lock/);
    assert.match(staged, /sdk\/typescript\/package\.json/u);
  } finally {
    rmSync(repo, { recursive: true, force: true });
  }
});

test("prepares a patch version when staged shipped code is deleted", () => {
  const repo = createReleaseRepository();
  try {
    rmSync(join(repo, "crates/blobyard-cli/src/removed.rs"));
    run("git", ["add", "crates/blobyard-cli/src/removed.rs"], repo);

    const prepared = run("node", [SCRIPT_PATH, "prepare"], repo);

    assert.match(prepared, /Prepared CLI release 0\.1\.10/);
    assert.equal(workspaceVersion(readFileSync(join(repo, "Cargo.toml"), "utf8")), "0.1.10");
  } finally {
    rmSync(repo, { recursive: true, force: true });
  }
});

test("allows docs-only tags and prepares the next free product version for later CLI work", () => {
  const repo = createReleaseRepository();
  try {
    mkdirSync(join(repo, "docs"), { recursive: true });
    writeFileSync(join(repo, "docs/release.md"), "# Release notes\n");
    run("git", ["add", "docs/release.md"], repo);
    run("git", ["commit", "--quiet", "-m", "docs-only release"], repo);
    run("git", ["tag", "v0.1.10"], repo);

    const platformOnly = run("node", [SCRIPT_PATH, "check"], repo);
    assert.match(platformOnly, /passed at 0\.1\.9; latest product release v0\.1\.10/u);

    writeFileSync(join(repo, "crates/blobyard-cli/src/lib.rs"), "pub fn value() -> u8 { 2 }\n");
    run("git", ["add", "crates/blobyard-cli/src/lib.rs"], repo);
    const prepared = run("node", [SCRIPT_PATH, "prepare"], repo);
    const checked = run("node", [SCRIPT_PATH, "check"], repo);

    assert.match(prepared, /Prepared CLI release 0\.1\.11/u);
    assert.match(checked, /passed at 0\.1\.11; latest product release v0\.1\.10/u);
    assert.equal(workspaceVersion(readFileSync(join(repo, "Cargo.toml"), "utf8")), "0.1.11");
  } finally {
    rmSync(repo, { recursive: true, force: true });
  }
});
