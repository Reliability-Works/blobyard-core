# blobyard-mcp

`blobyard-mcp` is Blobyard's private Model Context Protocol adapter. It reads one compact JSON-RPC
message per line from standard input and writes only compact JSON-RPC messages to standard output.
Diagnostics belong on standard error.

The crate does not authenticate independently. A host implements `ToolBackend` and uses Blobyard's
existing CLI session and authorization path. Backend results must contain user-facing metadata, not
raw tokens, cookies, authorization headers, OAuth codes, OTPs, confirmation codes, provider secrets,
or presigned URLs. An explicitly requested share, preview, or inbox creation may return only the
public capability URL it just issued. The adapter redacts those URL fields from every other tool,
resource, and error result, and applies a final defensive pass to all other sensitive material.

The supported MCP revision is `2025-11-25`. Tool inputs use JSON Schema 2020-12. Resource and
resource-template entries describe safe metadata views and direct clients to their authorized tools.
The `artifact_handoff` prompt helps a client plan a safe upload and share flow.

The tool catalog includes workspace list, create, and rename, project and object workflows,
capability management, retention, Web Yards, audit, members, invitations, billing state, account
export state, deletion state, redacted API-token metadata, GitHub OIDC trusts, and CLI-session
management. API-token creation, hosted Stripe sessions, signed account-export downloads, account
deletion preparation and completion, and one-time secret operations are intentionally absent because
their bearer credentials, redirects, signed URLs, confirmation capabilities, or secret material must
not enter model context. Destructive account deletion retry is also absent. OpenAPI records these
exclusions, and generated parity tests verify every declared MCP tool.

## Host integration

Implement `ToolBackend::call`, match on `ToolCall`, and return a JSON object or array suitable for
`structuredContent`. Call `serve_stdio` from the private CLI subcommand. Tests can use `serve` with
duplex or in-memory asynchronous I/O.
