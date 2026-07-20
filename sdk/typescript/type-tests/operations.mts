import { BlobYardClient } from "../src/index.mjs";

declare const client: BlobYardClient;

const created = client.operations.createProject({
  body: { name: "Artifacts", workspace: "example-team" },
});
const projects = client.operations.listProjects({ query: { workspace: "example-team" } });
const health = client.operations.health();
const upload = client.operations.requestUpload({
  body: {
    checksumSha256: "a".repeat(64),
    contentType: "text/plain",
    filename: "artifact.txt",
    path: "artifact.txt",
    project: "mobile",
    sizeBytes: 1,
    workspace: "example-team",
  },
  idempotencyKey: "upload-artifact",
});

void created;
void projects;
void health;
void upload;

// @ts-expect-error createProject requires its request body.
client.operations.createProject();
client.operations.createProject({
  body: { name: "Artifacts", workspace: "example-team" },
  // @ts-expect-error createProject does not accept list query parameters.
  query: { cursor: "next" },
});
client.operations.createProject({
  body: { name: "Artifacts", workspace: "example-team" },
  // @ts-expect-error createProject does not durably replay idempotency keys.
  idempotencyKey: "unsupported",
});
// @ts-expect-error requestUpload requires its retry-stable idempotency key.
client.operations.requestUpload({
  body: {
    checksumSha256: "a".repeat(64),
    contentType: "text/plain",
    filename: "artifact.txt",
    path: "artifact.txt",
    project: "mobile",
    sizeBytes: 1,
    workspace: "example-team",
  },
});
// @ts-expect-error the createProject response is not an opaque generic.
created.then((value) => value.items);
