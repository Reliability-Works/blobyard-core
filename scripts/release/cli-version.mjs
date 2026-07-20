#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

import { git, lines } from "./git.mjs";

const VERSION_PATTERN = /^(\d+)\.(\d+)\.(\d+)$/;
const WORKSPACE_VERSION_PATTERN = /(\[workspace\.package\][\s\S]*?\nversion = ")([^"]+)(")/;
const BASELINE_PATH = "release/baseline.json";
const SDK_PACKAGE_PATH = "sdk/typescript/package.json";

const EXACT_RELEASE_PATHS = new Set([
  ".github/actions/upload/action.yml",
  ".github/actions/upload/run.sh",
  ".github/workflows/release.yml",
  "Cargo.lock",
  "Cargo.toml",
  "crates/blobyard-api-client/Cargo.toml",
  "crates/blobyard-cli/Cargo.toml",
  "crates/blobyard-core/Cargo.toml",
  "crates/blobyard-mcp/Cargo.toml",
  "rust-toolchain.toml",
  "scripts/install.sh",
]);

const RELEASE_PREFIXES = [
  "conformance/",
  "crates/blobyard-api-client/",
  "crates/blobyard-cli/",
  "crates/blobyard-contract/",
  "crates/blobyard-core/",
  "crates/blobyard-mcp/",
  "crates/blobyard-repository-sqlite/",
  "crates/blobyard-server/",
  "crates/blobyard-storage-filesystem/",
  "crates/blobyard-storage-s3/",
  "deploy/",
  "openapi/",
  "packaging/docker/",
  "packaging/homebrew/",
  "release/",
  "sdk/typescript/",
];

export function isReleaseImpactingPath(path) {
  if (path.includes("/tests/") || path.startsWith("release/tests/")) {
    return false;
  }
  return (
    EXACT_RELEASE_PATHS.has(path) || RELEASE_PREFIXES.some((prefix) => path.startsWith(prefix))
  );
}

export function parseVersion(value) {
  const match = VERSION_PATTERN.exec(value);
  if (!match) {
    throw new Error(`Expected a semantic version, received ${value}.`);
  }
  return match.slice(1).map(Number);
}

export function compareVersions(left, right) {
  const leftParts = parseVersion(left);
  const rightParts = parseVersion(right);
  for (let index = 0; index < leftParts.length; index += 1) {
    const difference = leftParts[index] - rightParts[index];
    if (difference !== 0) {
      return Math.sign(difference);
    }
  }
  return 0;
}

export function nextPatchVersion(value) {
  const [major, minor, patch] = parseVersion(value);
  return `${major}.${minor}.${patch + 1}`;
}

export function workspaceVersion(source) {
  const match = WORKSPACE_VERSION_PATTERN.exec(source);
  if (!match) {
    throw new Error("Cargo.toml does not define [workspace.package] version.");
  }
  return match[2];
}

export function replaceWorkspaceVersion(source, version) {
  parseVersion(version);
  if (!WORKSPACE_VERSION_PATTERN.test(source)) {
    throw new Error("Cargo.toml does not define [workspace.package] version.");
  }
  return source.replace(WORKSPACE_VERSION_PATTERN, `$1${version}$3`);
}

export function sdkVersion(source) {
  const document = JSON.parse(source);
  if (typeof document?.version !== "string") {
    throw new Error("TypeScript SDK package does not define a version.");
  }
  parseVersion(document.version);
  return document.version;
}

export function replaceSdkVersion(source, version) {
  parseVersion(version);
  const document = JSON.parse(source);
  if (typeof document?.version !== "string") {
    throw new Error("TypeScript SDK package does not define a version.");
  }
  document.version = version;
  return `${JSON.stringify(document, null, 2)}\n`;
}

function baselineVersion(repo) {
  const document = JSON.parse(readFileSync(resolve(repo, BASELINE_PATH), "utf8"));
  if (
    document?.schemaVersion !== 1 ||
    document.previousRepository !== "Reliability-Works/blobyard" ||
    typeof document.previousVersion !== "string" ||
    typeof document.sourceRevision !== "string" ||
    !/^[0-9a-f]{40}$/u.test(document.sourceRevision)
  ) {
    throw new Error("Release baseline is invalid.");
  }
  parseVersion(document.previousVersion);
  return document.previousVersion;
}

function hasHead(repo) {
  try {
    git(repo, ["rev-parse", "--verify", "HEAD"]);
    return true;
  } catch {
    return false;
  }
}

function latestRelease(repo) {
  const tag = hasHead(repo)
    ? lines(
        git(repo, ["tag", "--merged", "HEAD", "--list", "v[0-9]*", "--sort=-version:refname"]),
      )[0]
    : undefined;
  if (!tag) {
    return { tag: null, version: baselineVersion(repo) };
  }
  const version = tag.slice(1);
  parseVersion(version);
  return { tag, version };
}

function stagedPaths(repo) {
  return lines(git(repo, ["diff", "--cached", "--name-only", "--no-renames"]));
}

function releaseChanges(repo, tag) {
  const committed = tag
    ? lines(git(repo, ["diff", "--name-only", "--no-renames", `${tag}..HEAD`]))
    : hasHead(repo)
      ? lines(git(repo, ["ls-files"]))
      : [];
  const staged = stagedPaths(repo);
  const working = lines(git(repo, ["diff", "--name-only", "--no-renames"]));
  return [...new Set([...committed, ...staged, ...working])].filter(isReleaseImpactingPath);
}

function readVersion(repo) {
  return workspaceVersion(readFileSync(resolve(repo, "Cargo.toml"), "utf8"));
}

function readSdkVersion(repo) {
  return sdkVersion(readFileSync(resolve(repo, SDK_PACKAGE_PATH), "utf8"));
}

function prepare(repo) {
  const impactful = stagedPaths(repo).filter(isReleaseImpactingPath);
  if (impactful.length === 0) {
    console.log("CLI release version unchanged: no staged release-impacting paths.");
    return;
  }

  const latest = latestRelease(repo);
  const current = readVersion(repo);
  const comparison = compareVersions(current, latest.version);
  if (comparison > 0) {
    console.log(
      `CLI release version ${current} is already ahead of ${latest.tag ?? `baseline v${latest.version}`}.`,
    );
    return;
  }

  const cargoPath = resolve(repo, "Cargo.toml");
  const lockPath = resolve(repo, "Cargo.lock");
  const sdkPath = resolve(repo, SDK_PACKAGE_PATH);
  const cargoSource = readFileSync(cargoPath, "utf8");
  const lockSource = readFileSync(lockPath, "utf8");
  const sdkSource = readFileSync(sdkPath, "utf8");
  const next = nextPatchVersion(latest.version);
  try {
    writeFileSync(cargoPath, replaceWorkspaceVersion(cargoSource, next));
    writeFileSync(sdkPath, replaceSdkVersion(sdkSource, next));
    execFileSync("cargo", ["update", "--workspace", "--offline"], {
      cwd: repo,
      stdio: ["ignore", "ignore", "inherit"],
    });
    git(repo, ["add", "Cargo.toml", "Cargo.lock", SDK_PACKAGE_PATH]);
  } catch (error) {
    writeFileSync(cargoPath, cargoSource);
    writeFileSync(lockPath, lockSource);
    writeFileSync(sdkPath, sdkSource);
    throw error;
  }
  console.log(
    `Prepared CLI release ${next} for ${impactful.length} staged release-impacting path(s).`,
  );
}

function check(repo) {
  const latest = latestRelease(repo);
  const current = readVersion(repo);
  const currentSdk = readSdkVersion(repo);
  if (currentSdk !== current) {
    throw new Error(
      `TypeScript SDK version ${currentSdk} does not match Core release version ${current}.`,
    );
  }
  const impactful = releaseChanges(repo, latest.tag);
  if (impactful.length > 0 && compareVersions(current, latest.version) <= 0) {
    throw new Error(
      `CLI release-impacting changes require a version newer than ${latest.tag ?? `baseline v${latest.version}`}:\n${impactful.join("\n")}`,
    );
  }
  console.log(
    `CLI release version contract passed at ${current}; latest product release ${latest.tag ?? `baseline v${latest.version}`}.`,
  );
}

function main() {
  const command = process.argv[2];
  const repo = resolve(process.cwd());
  if (command === "prepare") {
    prepare(repo);
    return;
  }
  if (command === "check") {
    check(repo);
    return;
  }
  throw new Error("Usage: cli-version.mjs prepare | check");
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
