# Security policy

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability. Email `security@reliability.works` with
the affected version, reproduction steps, impact, and any suggested mitigation. Do not include live
credentials or user data.

We will acknowledge a complete report, investigate it, and coordinate remediation and disclosure.

## Security contract

- Users, CLIs, CI jobs, and agents do not receive permanent storage credentials.
- Capabilities are scoped, short-lived, revocable, and auditable.
- Raw capabilities and bootstrap tokens are returned once and stored only as hashes.
- Secrets must not appear in logs, screenshots, tests, list APIs, or command output.
- Authorization and resource limits are enforced by the server on every path.
- User HTML is served from an isolated origin, not from an authenticated management origin.
- Release artifacts are checksummed, signed, and bound to GitHub provenance before installation.

## Generic OIDC sign-in

Self-hosted generic OIDC is an optional browser authentication path, not an identity-provisioning or
authorization path. Core binds an exact issuer and provider subject only to one pre-existing active
local user in the Yard workspace or one accepted guest with a current grant. Local-user emails are
stored trimmed and lowercased so the provider-verified email compares exactly. Zero or ambiguous
matches fail closed. A later missing verified email or email drift denies sign-in and revokes the
bound identity's active Yard sessions.

The issuer, client identifier, and client secret are all required together. The client secret is
accepted only through `BLOBYARD_OIDC_CLIENT_SECRET`; there is no command-line secret flag. Provider
discovery and endpoint validation complete before the listener opens. The derived callback origin
must use HTTPS, except for loopback HTTP origins in local development and tests, and an insecure
callback origin stops startup before any provider request. Authorization uses fixed
`openid email profile` scopes, PKCE S256, a nonce, hashed single-use state, and exact callback and
tenant context. Core validates issuer, audience and authorized party, signature, expiry, not-before,
nonce, access-token hash, subject consistency, and verified email before binding.

Every provider URL, including the discovered key set, must pass the same secure endpoint policy
before any request executes: HTTPS everywhere, HTTP only for loopback hosts, and no credentials or
fragments. Provider response bodies are bounded at four MiB against both declared and streamed
lengths, so a misbehaving provider cannot exhaust server memory.

Provider tokens, the client secret, raw state, nonce, and PKCE verifier are not persisted. The
browser-only `/account/yard-oidc/start` and `/account/yard-oidc/callback` routes are intentionally
absent from OpenAPI, SDK, CLI, and MCP surfaces.

The repository runs secret scanning, dependency policy, strict static analysis, complete coverage,
and fail-closed operator acceptance as release-blocking gates.
