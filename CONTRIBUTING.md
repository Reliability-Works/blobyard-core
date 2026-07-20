# Contributing

Blob Yard Core is currently private while its extraction and release contract are validated.
Contribution workflows will open with the repository. Until then, authorized contributors should use
a short-lived branch and a pull request into `main`.

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
