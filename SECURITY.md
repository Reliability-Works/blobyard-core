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

The repository runs secret scanning, dependency policy, strict static analysis, complete coverage,
and fail-closed operator acceptance as release-blocking gates.
