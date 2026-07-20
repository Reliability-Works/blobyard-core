#!/usr/bin/env node

import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

const PUBLIC_RELEASE_ORIGIN = "https://releases.blobyard.com";
const RELEASE_MANIFEST_NAME = "blobyard-release-manifest.json";
const RELEASE_REPOSITORY = "Reliability-Works/blobyard-core";
const SIGNING_ISSUER = "https://token.actions.githubusercontent.com";
const SIGNING_WORKFLOW = ".github/workflows/release.yml";
const VERSION_PATTERN = /^[0-9]+\.[0-9]+\.[0-9]+(?:[.-][0-9A-Za-z.-]+)?$/u;
const DEFAULT_REQUEST_TIMEOUT_MS = 15_000;
const INITIAL_BACKOFF_MS = 5_000;
const MAX_BACKOFF_MS = 30_000;

function object(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value : null;
}

function releaseVersion(value) {
  if (typeof value !== "string" || !VERSION_PATTERN.test(value)) {
    throw new Error(`CLI release version is invalid: ${String(value)}`);
  }
  return value;
}

function safeAssetName(value) {
  return typeof value === "string" && /^[A-Za-z0-9_-][A-Za-z0-9._-]*$/u.test(value);
}

export function workspaceCliVersion(source) {
  let inWorkspacePackage = false;
  for (const line of source.split(/\r?\n/u)) {
    const section = /^\s*\[([^[]+)\]\s*(?:#.*)?$/u.exec(line);
    if (section !== null) {
      if (inWorkspacePackage) break;
      inWorkspacePackage = section[1] === "workspace.package";
      continue;
    }
    if (!inWorkspacePackage) continue;
    const version = /^\s*version\s*=\s*"([^"]+)"\s*(?:#.*)?$/u.exec(line);
    if (version !== null) return releaseVersion(version[1]);
  }
  throw new Error("Cargo.toml does not define [workspace.package] version.");
}

export function requiredInstallerAssets(manifest, expectedVersion) {
  const root = object(manifest);
  const signing = object(root?.signing);
  const assets = object(root?.assets);
  if (
    root?.schemaVersion !== 1 ||
    root.repository !== RELEASE_REPOSITORY ||
    root.version !== expectedVersion ||
    signing?.oidcIssuer !== SIGNING_ISSUER ||
    signing.workflow !== SIGNING_WORKFLOW
  ) {
    throw new Error(`release manifest does not describe trusted Blob Yard CLI ${expectedVersion}`);
  }
  const platform = Array.isArray(root.platforms)
    ? root.platforms.find((candidate) => object(candidate)?.key === "linux-amd64")
    : undefined;
  const names = [
    assets?.checksums,
    assets?.checksumsSignature,
    assets?.provenance,
    object(platform)?.archive,
  ];
  if (!names.every(safeAssetName)) {
    throw new Error("release manifest does not contain safe Linux installer assets");
  }
  return names;
}

function responseError(response, label) {
  return `${label} returned HTTP ${String(response.status)}`;
}

async function publicRequest(fetchImpl, url, init, deadlineMs, now, requestTimeoutMs) {
  const remainingMs = deadlineMs - now();
  if (remainingMs <= 0) throw new Error("release availability request exceeded the wait deadline");
  return fetchImpl(url, {
    ...init,
    headers: { "cache-control": "no-cache" },
    signal: AbortSignal.timeout(Math.min(requestTimeoutMs, remainingMs)),
  });
}

export async function checkExactCliRelease(
  version,
  {
    deadlineMs = Number.POSITIVE_INFINITY,
    fetchImpl = fetch,
    now = Date.now,
    origin = PUBLIC_RELEASE_ORIGIN,
    requestTimeoutMs = DEFAULT_REQUEST_TIMEOUT_MS,
  } = {},
) {
  const expectedVersion = releaseVersion(version);
  const base = `${origin}/v${expectedVersion}`;
  const manifestUrl = `${base}/${RELEASE_MANIFEST_NAME}`;
  const response = await publicRequest(
    fetchImpl,
    manifestUrl,
    { method: "GET" },
    deadlineMs,
    now,
    requestTimeoutMs,
  );
  if (!response.ok) throw new Error(responseError(response, "release manifest"));
  let manifest;
  try {
    manifest = await response.json();
  } catch {
    throw new Error("release manifest is not valid JSON");
  }
  const assets = requiredInstallerAssets(manifest, expectedVersion);
  for (const asset of assets) {
    const assetResponse = await publicRequest(
      fetchImpl,
      `${base}/${asset}`,
      { method: "HEAD" },
      deadlineMs,
      now,
      requestTimeoutMs,
    );
    if (!assetResponse.ok) throw new Error(responseError(assetResponse, `release asset ${asset}`));
  }
}

function errorMessage(error) {
  return error instanceof Error ? error.message : "release availability check failed";
}

function timeoutError(version, timeoutMs, attempts, lastFailure) {
  return new Error(
    `Timed out after ${String(Math.ceil(timeoutMs / 1_000))} seconds waiting for exact public Blob Yard CLI ${version} after ${String(attempts)} attempts. Last check: ${lastFailure}. The acceptance run did not fall back to latest.`,
  );
}

export async function waitForExactCliRelease(
  version,
  timeoutMs,
  {
    check = checkExactCliRelease,
    initialBackoffMs = INITIAL_BACKOFF_MS,
    maxBackoffMs = MAX_BACKOFF_MS,
    now = Date.now,
    onRetry = ({ delayMs, failure, version: expectedVersion }) => {
      process.stderr.write(
        `Blob Yard CLI ${expectedVersion} is not public yet: ${failure}. Retrying in ${String(Math.ceil(delayMs / 1_000))} seconds.\n`,
      );
    },
    sleep = (delayMs) => new Promise((resolve) => setTimeout(resolve, delayMs)),
  } = {},
) {
  const expectedVersion = releaseVersion(version);
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs <= 0) {
    throw new Error("CLI release wait timeout must be a positive integer in milliseconds.");
  }
  const startedAt = now();
  const deadlineMs = startedAt + timeoutMs;
  let attempts = 0;
  let lastFailure = "no availability check completed";
  while (now() < deadlineMs) {
    attempts += 1;
    try {
      await check(expectedVersion, { deadlineMs, now });
      return;
    } catch (error) {
      lastFailure = errorMessage(error);
    }
    const remainingMs = deadlineMs - now();
    if (remainingMs <= 0) break;
    const exponent = Math.min(attempts - 1, 20);
    const delayMs = Math.min(initialBackoffMs * 2 ** exponent, maxBackoffMs, remainingMs);
    onRetry({ attempts, delayMs, failure: lastFailure, version: expectedVersion });
    await sleep(delayMs);
  }
  throw timeoutError(expectedVersion, timeoutMs, attempts, lastFailure);
}

function usage() {
  return "Usage: cli-release.mjs version [Cargo.toml] | wait <version> <timeout-seconds>";
}

async function main(arguments_) {
  const [command, ...values] = arguments_;
  if (command === "version" && values.length <= 1) {
    const source = await readFile(values[0] ?? "Cargo.toml", "utf8");
    process.stdout.write(`${workspaceCliVersion(source)}\n`);
    return;
  }
  if (command === "wait" && values.length === 2) {
    const timeoutSeconds = Number(values[1]);
    if (!Number.isSafeInteger(timeoutSeconds) || timeoutSeconds <= 0) {
      throw new Error("CLI release wait timeout must be a positive integer in seconds.");
    }
    await waitForExactCliRelease(values[0], timeoutSeconds * 1_000);
    process.stdout.write(`Exact public Blob Yard CLI ${values[0]} is available.\n`);
    return;
  }
  throw new Error(usage());
}

if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).catch((error) => {
    process.stderr.write(`${errorMessage(error)}\n`);
    process.exitCode = 1;
  });
}
