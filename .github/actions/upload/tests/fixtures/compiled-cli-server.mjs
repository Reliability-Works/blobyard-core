import { createHash } from "node:crypto";
import { writeFileSync } from "node:fs";
import { createServer } from "node:http";

const portFile = process.env.BLOBYARD_TEST_PORT_FILE;
if (portFile === undefined) throw new Error("BLOBYARD_TEST_PORT_FILE is required.");

let reservation;
let uploaded = Buffer.alloc(0);

function envelope(response, requestId, data, status = 200) {
  response.writeHead(status, {
    "content-type": "application/json",
    "x-request-id": requestId,
  });
  response.end(JSON.stringify({ data, ok: true, requestId }));
}

async function readJson(request) {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

function authorized(request) {
  return request.headers.authorization === "Bearer scoped-compiled-fixture";
}

async function apiRequest(request, response, origin) {
  if (!authorized(request)) {
    response.writeHead(401).end();
    return;
  }
  if (request.method === "GET" && request.url === "/v1/cli/whoami") {
    envelope(response, "req_whoami", {
      defaultWorkspace: { id: "workspace_1", name: "Acme", slug: "acme" },
      displayName: "GitHub Actions · acme/artifacts",
      principalId: "machine_1",
      principalType: "ci",
      scopes: ["upload"],
    });
    return;
  }
  if (request.method === "POST" && request.url === "/v1/uploads/request") {
    reservation = await readJson(request);
    envelope(response, "req_reserve", {
      expiresAt: "2030-01-01T00:15:00.000Z",
      headers: [],
      partSizeBytes: null,
      strategy: "single",
      uploadId: "upload_1",
      uploadUrl: `${origin}/signed-upload`,
    });
    return;
  }
  if (request.method === "POST" && request.url === "/v1/uploads/complete") {
    await readJson(request);
    const checksum = createHash("sha256").update(uploaded).digest("hex");
    if (reservation?.checksumSha256 !== checksum || reservation.sizeBytes !== uploaded.length) {
      response.writeHead(409).end();
      return;
    }
    envelope(response, "req_complete", {
      checksumSha256: checksum,
      sizeBytes: uploaded.length,
      uri: "blobyard://acme/demo/artifact.txt?version=7",
    });
    return;
  }
  response.writeHead(404).end();
}

const server = createServer(async (request, response) => {
  const address = server.address();
  if (address === null || typeof address === "string") throw new Error("Missing server address.");
  const origin = `http://127.0.0.1:${String(address.port)}`;
  if (request.method === "PUT" && request.url === "/signed-upload") {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    uploaded = Buffer.concat(chunks);
    response.writeHead(200).end();
    return;
  }
  await apiRequest(request, response, origin);
});

server.listen(0, "127.0.0.1", () => {
  const address = server.address();
  if (address === null || typeof address === "string") throw new Error("Missing server address.");
  writeFileSync(portFile, String(address.port), { mode: 0o600 });
});

process.on("SIGTERM", () => server.close());
