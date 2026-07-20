# Blob Yard TypeScript SDK

The SDK is a dependency-free, typed transport for the canonical Blob Yard OpenAPI contract. It uses
the same stable operation identifiers as `https://blobyard.com/openapi.json`.

```ts
import { BlobYardClient } from "@blobyard/sdk";

const client = new BlobYardClient({ accessToken: process.env.BLOBYARD_TOKEN });
const projects = await client.operations.listProjects({
  query: { workspace: "example-team" },
});
```

For a standalone deployment, set its API URL and declare the deployment type:

```ts
const local = new BlobYardClient({
  accessToken: process.env.BLOBYARD_TOKEN,
  baseUrl: "http://127.0.0.1:8787/v1",
  deployment: "self-hosted",
});
```

The default `cloud` deployment exposes both core and hosted-extension operations. A `self-hosted`
client rejects hosted-extension operations locally with `OPERATION_UNSUPPORTED` before calling
`fetch`.

Bindings derive operation-specific request bodies, query values, and result data from the canonical
OpenAPI schemas. They also type operation names, paths, cancellation, idempotency keys, and stable
errors. A binding rejects missing required input and fields that do not belong to that operation at
compile time. An `idempotencyKey` option exists only when the server durably replays that operation.

The client never persists or logs a token. Prefer a callback backed by the host's secret store when
tokens rotate. Public capability operations can be called without `accessToken`.
