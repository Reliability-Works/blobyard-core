# Contributing

Blob Yard Core accepts focused contributions that preserve its self-hostable product contract and
hard quality gates. Use a short-lived branch and open a pull request into `main`.

## Before opening a pull request

1. Keep the change within the self-hostable product scope described in [`README.md`](README.md) and
   [`ARCHITECTURE.md`](ARCHITECTURE.md).
2. Update implementation, tests, contracts, conformance evidence, and documentation together.
3. Run `./scripts/check.sh all`.
4. Explain the user-visible behavior, the failure path, and the exact validation run.

Do not lower a gate, broaden an exclusion, add a blanket suppression, or commit generated secrets.
Rust code denies unsafe code, warnings, panic-based normal control flow, production `unwrap` and
`expect`, oversized functions, and oversized source files. New executable behavior requires useful
tests at the layer that owns it.

## Licensing of contributions

Blob Yard Core is licensed under the Apache License 2.0. By submitting a pull request, you agree
that your contribution is licensed under the Apache License 2.0, as described in section 5 of the
license, and that you have the right to submit it.

Every commit must carry a Developer Certificate of Origin sign-off
(https://developercertificate.org). Add one with `git commit -s`, which appends a
`Signed-off-by:` line matching your commit author identity. Pull requests containing unsigned
commits will not be merged.
