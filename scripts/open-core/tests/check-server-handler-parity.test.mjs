import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { checkServerHandlerParity } from "../check-server-handler-parity.mjs";

test("proves every Core operation has implemented route evidence", async () => {
  const result = await checkServerHandlerParity({ requireComplete: true });
  assert.equal(result.core, 49);
  assert.equal(result.implemented, 49);
  assert.equal(result.partial, 0);
  assert.equal(result.missing, 0);
  assert.deepEqual(result.pending, []);
});

test("rejects missing, overlapping, unknown, and unsupported handler evidence", async () => {
  const ledger = JSON.parse(
    await readFile("conformance-source/server-handler-parity.json", "utf8"),
  );
  const missing = structuredClone(ledger);
  delete missing.implemented.startWebYardDeploy;
  await assert.rejects(
    checkServerHandlerParity({ ledgerDocument: missing }),
    /missing handler status/u,
  );

  const incomplete = structuredClone(ledger);
  delete incomplete.implemented.startWebYardDeploy;
  incomplete.missing.push("startWebYardDeploy");
  await assert.rejects(
    checkServerHandlerParity({ ledgerDocument: incomplete, requireComplete: true }),
    /Core server handlers are incomplete: startWebYardDeploy/u,
  );

  const overlapping = structuredClone(ledger);
  overlapping.partial.push("health");
  await assert.rejects(
    checkServerHandlerParity({ ledgerDocument: overlapping }),
    /classifications overlap/u,
  );

  const unknown = structuredClone(ledger);
  unknown.missing.push("unknownOperation");
  await assert.rejects(
    checkServerHandlerParity({ ledgerDocument: unknown }),
    /unknown operations/u,
  );

  const invalidEvidence = structuredClone(ledger);
  invalidEvidence.implemented.health = "openapi/blobyard-core.openapi.yaml";
  await assert.rejects(
    checkServerHandlerParity({ ledgerDocument: invalidEvidence }),
    /invalid handler source evidence/u,
  );
});
