# Blob Yard Core Vision

This document is the direction of the product. Read it before product work. Every bug fix, feature,
and reviewed change should move Blob Yard Core toward the product described here. Work that pulls
away from it needs an explicit decision recorded in this file, not a silent exception.

## What Blob Yard Core is

Blob Yard Core is the self-hostable storage layer for CI, agents, and release artifacts. Git keeps
the source. CI and agents produce the output. Blob Yard Core gives every build, binary, dataset, log
bundle, and generated file a durable, permissioned path to the person or system that needs it.

It is the open half of one user contract. The core gives one developer durable storage, scoped
automation, previews, Web Yards, recovery, and the canonical CLI without requiring a Blob Yard
account. Blob Yard Cloud is the managed service around the same contract and lives in a separate
proprietary repository. The two integrate through a pinned, checksummed conformance bundle, never
through copied editable source.

## North star

A developer should go from nothing to a completed handoff in minutes: install one binary or start
one container, upload a file, and hand it off with scoped, revocable access. If any step requires
reading a cloud provider's IAM documentation, the product has failed that user.

Files are durable until the owner deletes them or a retention policy removes them. A share can
expire without deleting its file. An upload inbox grants a bounded way to receive data without
granting access to everything else. These truths are the product. If an implementation detail makes
them hard to explain, fix the implementation, not the explanation.

## What belongs here

Accept work that strengthens the self-hostable contract:

- Durability, correctness, and recovery: storage adapters, backup, restore, reconciliation, and
  migration tooling.
- The native surfaces: the CLI, MCP server, typed API client, TypeScript SDK, and GitHub Action.
- Scoped access: shares, previews, inboxes, retention, and the authorization model behind them.
- Web Yards as static publishing backed by durable private storage, with immutable history and
  inspectable rollback.
- Operator experience: simpler installation, clearer failure messages, better defaults, and
  documentation that leads with the user journey.
- Conformance: anything that keeps the contract, the OpenAPI documents, and the conformance bundle
  provably identical.

## What does not belong here

Decline or redirect work that pulls the core away from its shape:

- Cloud-only concerns: hosted identity, billing, email delivery, collaboration, and production
  operations. Those belong to Blob Yard Cloud.
- Server-side application execution, background job platforms, databases beyond the embedded
  metadata store, or general-purpose compute for Web Yards.
- Desktop filesystem mounts, bidirectional sync, or native mobile applications.
- Enterprise SAML/SCIM and organization-scale identity features.
- Malware detection or a full secret-scanning service for uploaded bytes.
- General-purpose source control or package registry behavior.
- Identifying, cross-site, or advertising telemetry.
- An npm-distributed CLI: the shipped CLI is the standalone Rust binary.
- Windows binary distribution, until native hardware testing and an acceptable signing process
  exist.
- Anything that weakens upload security, validation, audit, or access control to gain a feature.

A good issue report names the user journey that breaks, the expected obvious behavior, and the
smallest reproduction. Issues outside this scope should be closed kindly with a pointer to the right
home.

## How to review

Review every change against this file first, then against correctness:

1. Scope: does the change strengthen the self-hostable contract above, or does it belong to Cloud or
   the non-goals list? Out-of-scope work is declined regardless of implementation quality.
2. Truthfulness: do the contract, implementation, tests, conformance evidence, and documentation all
   describe the same behavior? A change that updates one and not the others is incomplete.
3. The durability truths: does the change keep files durable until deleted, shares expirable without
   data loss, and inboxes bounded? Any regression here outranks every other concern.
4. The capability model: does the change preserve scoped, short-lived, revocable access with raw
   secrets returned once, stored as hashes, and never logged?
5. Operator simplicity: can one developer still install, run, back up, and restore the result
   without new mandatory infrastructure?

Mechanics such as branch shape, gate requirements, and pull request expectations live in
[`CONTRIBUTING.md`](CONTRIBUTING.md). The working agreement for agent contributors lives in
[`AGENTS.md`](AGENTS.md).

## How to use this document

- Feature work: state which part of the vision the feature advances. If it advances none, question
  it before building it.
- Bug priority: anything that breaks a durability truth, leaks a credential, or blocks the
  first-handoff journey outranks polish.
- Scope decisions: when a request contradicts this document, either change the request or change
  this document deliberately in the same piece of work.
