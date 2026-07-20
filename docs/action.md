# GitHub Action

Blobyard's action is a shell-only composite action at `.github/actions/upload/action.yml`. It
installs a verified native CLI release and obtains a short-lived machine identity through GitHub
OIDC. The `upload` operation stores one artifact and can create a share. The `deploy` operation
publishes one named Web Yard or every Yard configured in `.blobyard.toml`.

## Artifact upload

```yaml
permissions:
  contents: read
  id-token: write
  pull-requests: write

steps:
  - uses: actions/checkout@v7
  - uses: Reliability-Works/blobyard-core/.github/actions/upload@v1
    with:
      api-url: https://api.blobyard.com/v1
      path: ./dist
      project: mobile
      workspace: acme
      share: true
      expires: 7d
      comment-on-pr: true
      github-token: ${{ github.token }}
```

Required inputs are `api-url`, `path`, and `project`. `operation` defaults to `upload`. Optional
upload inputs are `web-yard-origin`, `workspace`, `share`, `expires`, `comment-on-pr`,
`pull-request-number`, `github-token`, an explicit scoped `token`, and `local-cli-path` for
repository tests. Upload outputs are `uri`, `version`, and optional `share-url`.

Because those outputs identify one immutable object version, `path` must resolve to exactly one
uploaded file. Archive a directory before passing it to this action; use the CLI directly when a
workflow intentionally uploads a directory as multiple objects. When `workspace` is omitted, the
action uses the workspace bound to the verified machine session.

## Web Yard deployment

Build the static output before the Blob Yard step, then select the `deploy` operation:

```yaml
permissions:
  contents: read
  id-token: write

steps:
  - uses: actions/checkout@v7
  - run: ./scripts/build-docs.sh
  - id: yard
    uses: Reliability-Works/blobyard-core/.github/actions/upload@v1
    with:
      operation: deploy
      api-url: https://api.blobyard.com/v1
      web-yard-origin: https://blobyard.app
      path: ./apps/docs/dist
      project: documentation
      workspace: example-team
      yard: product-docs
      clean-urls: true
      public: true
  - run: printf '%s\n' "${{ steps.yard.outputs.yard-url }}"
  - run: printf '%s\n' "${{ steps.yard.outputs.deployment-url }}"
```

Web Yard deployments require `public: true`. Set `yard` for one destination. Leave `yard` empty to
run `blobyard deploy --all` from `path`, which must contain the project `.blobyard.toml`. Optional
deployment inputs are `web-yard-origin`, `spa`, `clean-urls`, `comment-on-pr`,
`pull-request-number`, `github-token`, an explicit scoped `token`, and `local-cli-path`.

A single deployment outputs `yard-url`, `deployment-url`, `deploy-id`, `status`, and `results`.
`yard-url` remains the stable alias, while `deployment-url` selects the exact retained deployment. A
multi-Yard run uses `results`, a compact JSON array containing both URLs and the success or bounded
failure of every configured Yard. Mixed results leave successful Yards live and fail the action
after the complete array is written.

`web-yard-origin` defaults to the Cloud root `https://blobyard.app`. A self-hosted installation must
set its own trusted domain root. The action passes that origin to the CLI and rejects stable or
immutable deployment URLs that are not exact first-level subdomains of it.

## Authentication

The normal path requests GitHub's OIDC token and exchanges it at `POST /v1/ci/github/oidc/exchange`.
Blobyard verifies signature, exact issuer/audience, time claims, repository, owner, ref, workflow
path/ref, environment, project, and allowed actions before minting a machine token valid for no more
than 15 minutes.

The workflow must grant `id-token: write`. A missing permission or untrusted claim fails before
upload. The requested OIDC audience is the `api-url` origin without `/v1`, which must exactly match
`GITHUB_OIDC_AUDIENCE`. The explicit token input is a scoped fallback for systems that cannot issue
GitHub OIDC. Before uploading, the action calls `blobyard whoami` and requires a CI principal with
the requested upload/share scopes and workspace. Tokens are held only for the action process and are
never persisted or printed.

Web Yard deploys require both `upload` and `yard:manage`. The configured trust must grant both
actions for the selected repository, workflow, ref, workspace, and project.

On a `pull_request` event, `comment-on-pr: true` targets the pull request from the immutable GitHub
event payload. Workflows without that event, including `workflow_dispatch`, must also set
`pull-request-number` explicitly. The action validates that the positive integer identifies an
accessible pull request in `GITHUB_REPOSITORY` before reading or writing comments.

Workspace owners and administrators configure active trust rows through the authenticated Convex
functions in `ciTrustsApi`. A trust pins the repository, ref glob, workflow path and ref, optional
GitHub environment, optional project, and maximum allowed actions. Revoking a trust immediately
invalidates every machine session minted through it.

## Integrity

Release installation resolves the shared artifact manifest, downloads the target archive and
checksum material, verifies integrity before extraction, and executes only the expected `blobyard`
binary. A trusted `local-cli-path` bypass is available only when the caller supplied that path
explicitly, which keeps offline repository tests deterministic.

The action consumes stable JSON CLI output. It does not use a JavaScript action runtime or install
the product through a JavaScript package registry. Pull-request comments use `github-token` when it
is supplied and otherwise retain the existing `github.token` behavior. Pass `${{ github.token }}`
and grant `pull-requests: write` when comments are enabled. The action creates one bot-owned
Blobyard comment per pull request and updates that comment on later runs, so a commit series does
not accumulate stale artifact links. It never edits a matching comment created by another GitHub
identity.
