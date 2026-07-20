import assert from "node:assert/strict";
import test from "node:test";

import {
  checkExactCliRelease,
  requiredInstallerAssets,
  waitForExactCliRelease,
  workspaceCliVersion,
} from "../cli-release.mjs";

const VERSION = "1.2.3";

function releaseManifest(version = VERSION) {
  return {
    assets: {
      checksums: "SHA256SUMS",
      checksumsSignature: "SHA256SUMS.sig",
      provenance: "blobyard-provenance.intoto.jsonl",
    },
    platforms: [
      {
        archive: `blobyard-${version}-x86_64-unknown-linux-gnu.tar.gz`,
        key: "linux-amd64",
      },
    ],
    repository: "Reliability-Works/blobyard-core",
    schemaVersion: 1,
    signing: {
      oidcIssuer: "https://token.actions.githubusercontent.com",
      workflow: ".github/workflows/release.yml",
    },
    version,
  };
}

function response(status, body) {
  return new Response(body === undefined ? null : JSON.stringify(body), {
    headers: body === undefined ? {} : { "content-type": "application/json" },
    status,
  });
}

test("derives the CLI version from the Cargo workspace package", () => {
  assert.equal(
    workspaceCliVersion(`
[workspace]
members = ["crates/blobyard-cli"]

[workspace.package]
version = "1.2.3"
edition = "2024"
`),
    VERSION,
  );
  assert.throws(
    () => workspaceCliVersion('[workspace.package]\nversion = "latest"\n'),
    /version is invalid/u,
  );
  assert.throws(() => workspaceCliVersion("[workspace]\nresolver = 2\n"), /does not define/u);
});

test("requires the trusted exact manifest and every Linux installer asset", () => {
  assert.deepEqual(requiredInstallerAssets(releaseManifest(), VERSION), [
    "SHA256SUMS",
    "SHA256SUMS.sig",
    "blobyard-provenance.intoto.jsonl",
    "blobyard-1.2.3-x86_64-unknown-linux-gnu.tar.gz",
  ]);
  assert.throws(
    () => requiredInstallerAssets(releaseManifest("1.2.2"), VERSION),
    /does not describe trusted Blob Yard CLI 1\.2\.3/u,
  );
  const unsafe = releaseManifest();
  unsafe.assets.checksums = "../SHA256SUMS";
  assert.throws(
    () => requiredInstallerAssets(unsafe, VERSION),
    /does not contain safe Linux installer assets/u,
  );
});

test("waits with bounded backoff for the exact immutable release and never requests latest", async () => {
  let clock = 0;
  let manifestAttempts = 0;
  const delays = [];
  const requests = [];
  const fetchImpl = async (url, init) => {
    requests.push({ method: init.method, url });
    if (url.endsWith("blobyard-release-manifest.json")) {
      manifestAttempts += 1;
      return manifestAttempts < 3 ? response(404) : response(200, releaseManifest());
    }
    return response(200);
  };

  await waitForExactCliRelease(VERSION, 1_000, {
    check: async (version, options) => {
      await checkExactCliRelease(version, {
        ...options,
        fetchImpl,
        origin: "https://releases.example.test",
      });
    },
    initialBackoffMs: 100,
    maxBackoffMs: 200,
    now: () => clock,
    onRetry: () => {},
    sleep: async (delayMs) => {
      delays.push(delayMs);
      clock += delayMs;
    },
  });

  assert.deepEqual(delays, [100, 200]);
  assert.equal(manifestAttempts, 3);
  assert.equal(
    requests.every(({ url }) => url.includes(`/v${VERSION}/`)),
    true,
  );
  assert.equal(
    requests.some(({ url }) => url.includes("latest")),
    false,
  );
  assert.equal(
    requests.filter(({ method }) => method === "HEAD").length,
    requiredInstallerAssets(releaseManifest(), VERSION).length,
  );
});

test("proceeds immediately when the exact release already exists", async () => {
  let checks = 0;
  let sleeps = 0;
  await waitForExactCliRelease(VERSION, 1_000, {
    check: async () => {
      checks += 1;
    },
    now: () => 0,
    sleep: async () => {
      sleeps += 1;
    },
  });
  assert.equal(checks, 1);
  assert.equal(sleeps, 0);
});

test("fails precisely at the bounded deadline without accepting another version", async () => {
  let clock = 0;
  let attempts = 0;
  await assert.rejects(
    waitForExactCliRelease(VERSION, 250, {
      check: async () => {
        attempts += 1;
        throw new Error("release manifest returned HTTP 404");
      },
      initialBackoffMs: 100,
      maxBackoffMs: 100,
      now: () => clock,
      onRetry: () => {},
      sleep: async (delayMs) => {
        clock += delayMs;
      },
    }),
    /Timed out after 1 seconds waiting for exact public Blob Yard CLI 1\.2\.3 after 3 attempts.*did not fall back to latest/u,
  );
  assert.equal(clock, 250);
  assert.equal(attempts, 3);
});
