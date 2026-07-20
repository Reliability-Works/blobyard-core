import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  BlobYardApiError,
  BlobYardClient,
  operations,
} from "../../../sdk/typescript/src/index.mjs";
import { loadComposedContract } from "../contract-files.mjs";
import { operationOwnership } from "../operation-ownership.mjs";

const document = await loadComposedContract(process.cwd());
const ownership = JSON.parse(readFileSync("openapi/operation-ownership.json", "utf8"));

test("every operation has one explicit core or hosted owner", () => {
  const classified = operationOwnership(document, ownership);
  assert.equal(classified.size, 74);
  assert.equal(classified.get("exchangeBootstrapToken"), "core");
  assert.equal(classified.get("requestUpload"), "core");
  assert.equal(classified.get("createBillingPortal"), "hosted-extension");

  const missing = { ...ownership, core: ownership.core.filter((id) => id !== "requestUpload") };
  assert.throws(() => operationOwnership(document, missing), /missing ownership: requestUpload/u);

  const duplicate = { ...ownership, hostedExtension: [...ownership.hostedExtension, "health"] };
  assert.throws(() => operationOwnership(document, duplicate), /health has duplicate ownership/u);

  const stale = { ...ownership, internal: ["unknownOperation"] };
  assert.throws(() => operationOwnership(document, stale), /unknown operations: unknownOperation/u);
});

test("generated bindings send idempotency keys only for durable replay operations", async () => {
  const calls = [];
  const client = new BlobYardClient({
    accessToken: async () => "session_token",
    baseUrl: "https://example.test/v1/",
    fetch: async (url, init) => {
      calls.push({ init, url });
      return new Response(JSON.stringify({ data: { items: [] }, ok: true, requestId: "req_1" }), {
        headers: { "content-type": "application/json", "x-request-id": "req_1" },
        status: 200,
      });
    },
  });
  const created = await client.operations.createProject({
    body: { name: "Mobile", workspace: "team" },
    idempotencyKey: "unsupported_key",
  });
  await client.operations.requestAccountExport({ body: {}, idempotencyKey: "idem_1" });
  const listed = await client.operations.listObjects({
    query: { project: "mobile", versions: true, workspace: "team" },
  });
  await client.operations.clearRetention({
    query: { project: "mobile", workspace: "team" },
  });
  assert.deepEqual(created, { items: [] });
  assert.deepEqual(listed, { items: [] });
  assert.equal(calls.length, 4);
  assert.equal(calls[0].url, "https://example.test/v1/projects");
  assert.equal(calls[0].init.method, "POST");
  assert.equal(calls[0].init.headers.get("authorization"), "Bearer session_token");
  assert.equal(calls[0].init.headers.get("idempotency-key"), null);
  assert.equal(calls[0].init.body, JSON.stringify({ name: "Mobile", workspace: "team" }));
  assert.equal(calls[1].url, "https://example.test/v1/account/exports");
  assert.equal(calls[1].init.headers.get("idempotency-key"), "idem_1");
  assert.equal(
    calls[2].url,
    "https://example.test/v1/objects?project=mobile&versions=true&workspace=team",
  );
  assert.equal(calls[3].url, "https://example.test/v1/retention?project=mobile&workspace=team");
  assert.equal(calls[3].init.method, "DELETE");
  assert.equal(calls[3].init.body, undefined);
  assert.equal(operations.createProject.idempotency, false);
  assert.equal(operations.createProject.ownership, "core");
  assert.equal(operations.requestAccountExport.idempotency, true);
  assert.equal(operations.requestAccountExport.ownership, "hosted-extension");
});

test("public operations never resolve or send a configured token", async () => {
  let authorization;
  let tokenCalls = 0;
  const client = new BlobYardClient({
    accessToken: async () => {
      tokenCalls += 1;
      return "must_not_be_sent";
    },
    fetch: async (_url, init) => {
      authorization = init.headers.get("authorization");
      return new Response(JSON.stringify({ data: { status: "ok" }, ok: true, requestId: "req_2" }));
    },
  });
  assert.deepEqual(await client.operations.health(), { status: "ok" });
  assert.equal(authorization, null);
  assert.equal(tokenCalls, 0);
  await assert.rejects(() => client.request("notAnOperation"), /Unknown Blob Yard operation/u);
  assert.equal(operations.health.public, true);
  assert.equal(operations.health.successStatus, 200);
});

test("self-hosted clients reject hosted extensions before fetch", async () => {
  let calls = 0;
  const client = new BlobYardClient({
    deployment: "self-hosted",
    fetch: async () => {
      calls += 1;
      throw new Error("fetch must not run");
    },
  });
  await assert.rejects(
    () => client.operations.createBillingPortal({ body: {} }),
    (error) =>
      error instanceof BlobYardApiError &&
      error.code === "OPERATION_UNSUPPORTED" &&
      error.requestId === null &&
      error.status === null,
  );
  assert.equal(calls, 0);
  assert.throws(
    () => new BlobYardClient({ deployment: "unsupported" }),
    /deployment must be cloud or self-hosted/u,
  );
});

test("cloud clients reject self-hosted bootstrap before fetch", async () => {
  let calls = 0;
  const client = new BlobYardClient({
    fetch: async () => {
      calls += 1;
      throw new Error("fetch must not run");
    },
  });
  await assert.rejects(
    () => client.operations.exchangeBootstrapToken({ body: {} }),
    (error) =>
      error instanceof BlobYardApiError &&
      error.code === "OPERATION_UNSUPPORTED" &&
      error.requestId === null &&
      error.status === null,
  );
  assert.equal(calls, 0);
});

test("API base URLs require HTTPS except on loopback development hosts", async () => {
  const requested = [];
  const response = () =>
    new Response(JSON.stringify({ data: { status: "ok" }, ok: true, requestId: "req_url" }));
  for (const baseUrl of [
    "https://example.test",
    "http://localhost:3210/v1/",
    "http://127.0.0.1:3210/v1",
    "http://[::1]:3210/v1",
  ]) {
    const client = new BlobYardClient({
      baseUrl,
      fetch: async (url) => {
        requested.push(url);
        return response();
      },
    });
    await client.operations.health();
  }
  assert.deepEqual(requested, [
    "https://example.test/v1/health",
    "http://localhost:3210/v1/health",
    "http://127.0.0.1:3210/v1/health",
    "http://[::1]:3210/v1/health",
  ]);
  for (const baseUrl of [
    "http://example.test/v1",
    "https://user:password@example.test/v1",
    "https://example.test/v2",
    "https://example.test/v1?tenant=other",
    "https://example.test/v1#other",
  ]) {
    assert.throws(
      () => new BlobYardClient({ baseUrl }),
      /API URL must be HTTPS, or HTTP on a loopback development host/u,
    );
  }
});

test("stable API errors and malformed responses reject without exposing request data", async () => {
  const rejected = new BlobYardClient({
    fetch: async () =>
      new Response(
        JSON.stringify({
          error: { code: "FORBIDDEN", message: "Scope denied." },
          ok: false,
          requestId: "req_denied",
        }),
        { status: 403 },
      ),
  });
  await assert.rejects(
    () => rejected.operations.listProjects(),
    (error) =>
      error instanceof BlobYardApiError &&
      error.code === "FORBIDDEN" &&
      error.requestId === "req_denied" &&
      error.status === 403,
  );
  const malformed = new BlobYardClient({
    fetch: async () => new Response("upstream html", { status: 502 }),
  });
  await assert.rejects(
    () => malformed.operations.health(),
    (error) => error instanceof BlobYardApiError && error.code === "INVALID_RESPONSE",
  );
});

test("success-shaped envelopes reject for HTTP errors and unexpected success statuses", async () => {
  for (const status of [500, 201]) {
    const client = new BlobYardClient({
      fetch: async () =>
        new Response(
          JSON.stringify({ data: { status: "ok" }, ok: true, requestId: `req_${status}` }),
          { status },
        ),
    });
    await assert.rejects(
      () => client.operations.health(),
      (error) =>
        error instanceof BlobYardApiError &&
        error.code === "INVALID_RESPONSE" &&
        error.requestId === `req_${status}` &&
        error.status === status,
    );
  }
});
