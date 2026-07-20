#!/usr/bin/env bash

set -euo pipefail

action_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
fixture_root="$action_root/tests/fixtures"
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
fixture="$temporary/artifact.txt"
printf 'artifact\n' >"$fixture"
site="$temporary/site"
mkdir -p "$site"
printf '<h1>fixture</h1>\n' >"$site/index.html"

assert_absent() {
  local value=$1 file=$2
  if grep -Fq "$value" "$file"; then
    printf 'sensitive fixture value appeared in action output\n' >&2
    exit 1
  fi
}

run_action() {
  GITHUB_ACTION_PATH="$action_root" \
    GITHUB_REPOSITORY=acme/artifacts \
    GITHUB_OUTPUT="$temporary/output" \
    RUNNER_TEMP="$temporary/runner" \
    BLOBYARD_ACTION_API_URL=https://api.blobyard.com/v1 \
    BLOBYARD_ACTION_WEB_YARD_ORIGIN="${BLOBYARD_ACTION_WEB_YARD_ORIGIN:-https://blobyard.app}" \
    BLOBYARD_ACTION_COMMENT="${BLOBYARD_ACTION_COMMENT:-false}" \
    BLOBYARD_ACTION_EXPIRES=7d \
    BLOBYARD_ACTION_GITHUB_TOKEN="${BLOBYARD_ACTION_GITHUB_TOKEN-workflow-fixture-token}" \
    BLOBYARD_ACTION_LOCAL_CLI="$fixture_root/blobyard" \
    BLOBYARD_ACTION_OPERATION="${BLOBYARD_ACTION_OPERATION:-upload}" \
    BLOBYARD_ACTION_PATH="${BLOBYARD_ACTION_PATH:-$fixture}" \
    BLOBYARD_ACTION_PR_NUMBER="${BLOBYARD_ACTION_PR_NUMBER:-}" \
    BLOBYARD_ACTION_PROJECT=demo \
    BLOBYARD_ACTION_SHARE="$1" \
    BLOBYARD_ACTION_SPA="${BLOBYARD_ACTION_SPA:-false}" \
    BLOBYARD_ACTION_CLEAN_URLS="${BLOBYARD_ACTION_CLEAN_URLS:-false}" \
    BLOBYARD_ACTION_PUBLIC="${BLOBYARD_ACTION_PUBLIC:-false}" \
    BLOBYARD_ACTION_TOKEN="${2:-}" \
    BLOBYARD_ACTION_WORKSPACE="${BLOBYARD_ACTION_WORKSPACE-acme}" \
    BLOBYARD_ACTION_YARD="${BLOBYARD_ACTION_YARD:-}" \
    MOCK_PRINCIPAL_TYPE="${MOCK_PRINCIPAL_TYPE:-ci}" \
    MOCK_SCOPES="${MOCK_SCOPES:-[\"upload\",\"share\",\"yard:manage\"]}" \
    MOCK_DEPLOY_MODE="${MOCK_DEPLOY_MODE:-success}" \
    MOCK_DEPLOY_ORIGIN="${MOCK_DEPLOY_ORIGIN:-https://blobyard.app}" \
    MOCK_UPLOAD_MODE="${MOCK_UPLOAD_MODE:-single}" \
    MOCK_WORKSPACE="${MOCK_WORKSPACE:-acme}" \
    "$action_root/run.sh"
}

: >"$temporary/output"
run_action true scoped-fixture-token 2>"$temporary/explicit.log"
grep -Fxq 'uri=blobyard://acme/demo/build.zip?version=7' "$temporary/output"
grep -Fxq 'version=7' "$temporary/output"
grep -Fxq 'share-url=https://blobyard.com/s/public-fixture' "$temporary/output"
grep -Fxq 'yard-url=' "$temporary/output"
grep -Fxq 'deployment-url=' "$temporary/output"

grep -Fxq 'deploy-id=' "$temporary/output"
grep -Fxq 'status=' "$temporary/output"
grep -Fxq 'results=[]' "$temporary/output"
assert_absent scoped-fixture-token "$temporary/explicit.log"

if run_action false '' 2>"$temporary/missing.log"; then
  printf 'action accepted missing token and OIDC permission\n' >&2
  exit 1
fi
grep -Fq 'permissions: id-token: write' "$temporary/missing.log"

: >"$temporary/output"
PATH="$fixture_root:$PATH" \
  ACTIONS_ID_TOKEN_REQUEST_URL=https://token.actions.test/request \
  ACTIONS_ID_TOKEN_REQUEST_TOKEN=request-fixture \
  CURL_ARGS_CAPTURE="$temporary/curl-args" \
  CURL_BODY_CAPTURE="$temporary/body.json" \
  run_action false '' 2>"$temporary/oidc.log"
jq -e '.actions == ["upload"] and .project == "demo" and .workspace == "acme"' "$temporary/body.json" >/dev/null
grep -Fq 'audience=https://api.blobyard.com' "$temporary/curl-args"
grep -Fq 'Authorization: Bearer oidc-fixture' "$temporary/curl-args"
grep -Fq 'https://api.blobyard.com/v1/ci/github/oidc/exchange' "$temporary/curl-args"
grep -Fxq 'share-url=' "$temporary/output"
assert_absent oidc-fixture "$temporary/oidc.log"
assert_absent machine-fixture "$temporary/oidc.log"

: >"$temporary/output"
BLOBYARD_ACTION_OPERATION=deploy \
  BLOBYARD_ACTION_PATH="$site" \
  BLOBYARD_ACTION_PUBLIC=true \
  BLOBYARD_ACTION_YARD=documentation \
  BLOBYARD_ACTION_SPA=true \
  BLOBYARD_ACTION_CLEAN_URLS=true \
  run_action false scoped-fixture-token 2>"$temporary/deploy.log"
grep -Fxq 'yard-url=https://documentation-123456789-acme.blobyard.app' "$temporary/output"
grep -Fxq 'deployment-url=https://documentation-0123456789-acme.blobyard.app' "$temporary/output"
grep -Fxq 'deploy-id=deploy_documentation' "$temporary/output"
grep -Fxq 'status=live' "$temporary/output"
jq -e 'length == 1 and .[0].yard == "documentation" and .[0].ok == true' \
  <<<"$(sed -n 's/^results=//p' "$temporary/output")" >/dev/null
assert_absent scoped-fixture-token "$temporary/deploy.log"

: >"$temporary/output"
BLOBYARD_ACTION_OPERATION=deploy \
  BLOBYARD_ACTION_PATH="$site" \
  BLOBYARD_ACTION_PUBLIC=true \
  BLOBYARD_ACTION_YARD=documentation \
  BLOBYARD_ACTION_WEB_YARD_ORIGIN=https://yards.example.test \
  MOCK_DEPLOY_ORIGIN=https://yards.example.test \
  run_action false scoped-fixture-token 2>"$temporary/self-hosted-deploy.log"
grep -Fxq 'yard-url=https://documentation-123456789-acme.yards.example.test' "$temporary/output"
grep -Fxq 'deployment-url=https://documentation-0123456789-acme.yards.example.test' "$temporary/output"

: >"$temporary/output"
PATH="$fixture_root:$PATH" \
  ACTIONS_ID_TOKEN_REQUEST_URL=https://token.actions.test/request \
  ACTIONS_ID_TOKEN_REQUEST_TOKEN=request-fixture \
  CURL_ARGS_CAPTURE="$temporary/deploy-curl-args" \
  CURL_BODY_CAPTURE="$temporary/deploy-body.json" \
  BLOBYARD_ACTION_OPERATION=deploy \
  BLOBYARD_ACTION_PATH="$site" \
  BLOBYARD_ACTION_PUBLIC=true \
  BLOBYARD_ACTION_YARD='' \
  run_action false '' 2>"$temporary/deploy-oidc.log"
jq -e '.actions == ["yard:manage", "upload"] and .project == "demo"' "$temporary/deploy-body.json" >/dev/null
jq -e 'length == 2 and .[0].yard == "dashboard" and .[1].yard == "documentation"' \
  <<<"$(sed -n 's/^results=//p' "$temporary/output")" >/dev/null
grep -Fxq 'yard-url=' "$temporary/output"
grep -Fxq 'deployment-url=' "$temporary/output"

expect_failure() {
  local expected=$1
  shift
  : >"$temporary/output"
  if "$@" 2>"$temporary/failure.log"; then
    printf 'action unexpectedly accepted an invalid fixture\n' >&2
    exit 1
  fi
  grep -Fq "$expected" "$temporary/failure.log"
}

BLOBYARD_ACTION_WEB_YARD_ORIGIN=https://yards.example.test/path \
  expect_failure 'web-yard-origin must be an HTTPS domain root' run_action false scoped-token

: >"$temporary/output"
BLOBYARD_ACTION_OPERATION=deploy BLOBYARD_ACTION_PUBLIC=true BLOBYARD_ACTION_PATH="$site" \
  BLOBYARD_ACTION_YARD='' MOCK_DEPLOY_MODE=mixed \
  expect_failure 'one or more Web Yard deploys failed' run_action false scoped-token
jq -e 'length == 2 and .[0].ok == true and .[1].ok == false and .[1].error.code == "PLAN_LIMIT"' \
  <<<"$(sed -n 's/^results=//p' "$temporary/output")" >/dev/null

BLOBYARD_ACTION_OPERATION=deploy BLOBYARD_ACTION_PUBLIC=false \
  BLOBYARD_ACTION_PATH="$site" expect_failure 'public must be true' run_action false scoped-token
BLOBYARD_ACTION_OPERATION=deploy BLOBYARD_ACTION_PUBLIC=true BLOBYARD_ACTION_PATH="$site" \
  MOCK_SCOPES='["upload"]' \
  expect_failure 'scoped Blobyard CI machine token' run_action false scoped-token
BLOBYARD_ACTION_OPERATION=deploy BLOBYARD_ACTION_PUBLIC=true BLOBYARD_ACTION_PATH="$site" \
  MOCK_SCOPES='["yard:manage"]' \
  expect_failure 'scoped Blobyard CI machine token' run_action false scoped-token
BLOBYARD_ACTION_OPERATION=deploy BLOBYARD_ACTION_PUBLIC=true BLOBYARD_ACTION_PATH="$fixture" \
  expect_failure 'path must be a directory' run_action false scoped-token
MOCK_PRINCIPAL_TYPE=cli expect_failure 'scoped Blobyard CI machine token' run_action false scoped-token
MOCK_SCOPES='["upload"]' expect_failure 'scoped Blobyard CI machine token' run_action true scoped-token
MOCK_WORKSPACE=other expect_failure 'requested workspace' run_action false scoped-token
MOCK_UPLOAD_MODE=multi expect_failure 'exactly one uploaded file' run_action false scoped-token
MOCK_UPLOAD_MODE=mutable expect_failure 'without an immutable version' run_action false scoped-token
: >"$temporary/output"
BLOBYARD_ACTION_WORKSPACE='' run_action false scoped-token 2>"$temporary/default-workspace.log"
grep -Fxq 'uri=blobyard://acme/demo/build.zip?version=7' "$temporary/output"

: >"$temporary/output"
PATH="$fixture_root:$PATH" \
  BLOBYARD_ACTION_COMMENT=true \
  BLOBYARD_ACTION_GITHUB_TOKEN=dedicated-comment-fixture-token \
  GH_CAPTURE="$temporary/gh-args" \
  GH_EXPECTED_TOKEN=dedicated-comment-fixture-token \
  GITHUB_EVENT_PATH="$fixture_root/pull-request.json" \
  run_action true scoped-token 2>"$temporary/comment.log"
grep -Fq 'issues/42/comments --method POST' "$temporary/gh-args"
grep -Fq 'pulls/42 --jq .number' "$temporary/gh-args"
grep -Fq 'blobyard://acme/demo/build.zip?version=7' "$temporary/gh-args"
grep -Fq 'https://blobyard.com/s/public-fixture' "$temporary/gh-args"
assert_absent dedicated-comment-fixture-token "$temporary/gh-args"
assert_absent dedicated-comment-fixture-token "$temporary/comment.log"

: >"$temporary/output"
: >"$temporary/gh-args"
PATH="$fixture_root:$PATH" \
  BLOBYARD_ACTION_COMMENT=true \
  BLOBYARD_ACTION_OPERATION=deploy \
  BLOBYARD_ACTION_PATH="$site" \
  BLOBYARD_ACTION_PUBLIC=true \
  BLOBYARD_ACTION_YARD=documentation \
  GH_CAPTURE="$temporary/gh-args" \
  GITHUB_EVENT_PATH="$fixture_root/pull-request.json" \
  run_action false scoped-token 2>"$temporary/deploy-comment.log"
grep -Fq 'https://documentation-123456789-acme.blobyard.app' "$temporary/gh-args"
grep -Fq 'https://documentation-0123456789-acme.blobyard.app' "$temporary/gh-args"
grep -Fq 'deploy_documentation' "$temporary/gh-args"

BLOBYARD_ACTION_COMMENT=true BLOBYARD_ACTION_GITHUB_TOKEN='' \
  expect_failure 'comment-on-pr requires github-token or github.token' run_action false scoped-token

: >"$temporary/output"
: >"$temporary/gh-args"
PATH="$fixture_root:$PATH" \
  BLOBYARD_ACTION_COMMENT=true \
  BLOBYARD_ACTION_PR_NUMBER=17 \
  GH_CAPTURE="$temporary/gh-args" \
  run_action false scoped-token 2>"$temporary/explicit-comment.log"
grep -Fq 'pulls/17 --jq .number' "$temporary/gh-args"
grep -Fq 'issues/17/comments --method POST' "$temporary/gh-args"

BLOBYARD_ACTION_COMMENT=true BLOBYARD_ACTION_PR_NUMBER=invalid \
  expect_failure 'pull-request-number must be a positive integer' run_action false scoped-token
PATH="$fixture_root:$PATH" BLOBYARD_ACTION_COMMENT=true BLOBYARD_ACTION_PR_NUMBER=404 \
  GH_CAPTURE="$temporary/gh-args" GH_INVALID_PR=true \
  expect_failure 'did not identify an accessible pull request' run_action false scoped-token
BLOBYARD_ACTION_COMMENT=true GITHUB_EVENT_PATH="$temporary/missing-event.json" \
  expect_failure 'pull_request event or pull-request-number' run_action false scoped-token

: >"$temporary/output"
: >"$temporary/gh-args"
PATH="$fixture_root:$PATH" \
  BLOBYARD_ACTION_COMMENT=true \
  GH_CAPTURE="$temporary/gh-args" \
  GH_EXISTING_COMMENT=true \
  GITHUB_EVENT_PATH="$fixture_root/pull-request.json" \
  run_action false scoped-token 2>"$temporary/comment-update.log"
grep -Fq 'issues/comments/91 --method PATCH' "$temporary/gh-args"

: >"$temporary/output"
: >"$temporary/gh-args"
PATH="$fixture_root:$PATH" \
  BLOBYARD_ACTION_COMMENT=true \
  GH_CAPTURE="$temporary/gh-args" \
  GH_EXISTING_FOREIGN_COMMENT=true \
  GITHUB_EVENT_PATH="$fixture_root/pull-request.json" \
  run_action false scoped-token 2>"$temporary/comment-foreign.log"
grep -Fq 'issues/42/comments --method POST' "$temporary/gh-args"
if grep -Fq 'issues/comments/92 --method PATCH' "$temporary/gh-args"; then
  printf 'action edited a marker comment owned by another identity\n' >&2
  exit 1
fi

for field in operation path project api-url web-yard-origin workspace yard spa clean-urls public token share expires comment-on-pr pull-request-number github-token local-cli-path; do
  grep -Eq "^  $field:" "$action_root/action.yml"
done
grep -Fq "BLOBYARD_ACTION_GITHUB_TOKEN: \${{ inputs.github-token || github.token }}" \
  "$action_root/action.yml"
if grep -Eq '^[[:space:]]+GH_TOKEN:' "$action_root/action.yml"; then
  printf 'action exposed the GitHub token before the pull request comment phase\n' >&2
  exit 1
fi
for field in uri version share-url yard-url deployment-url deploy-id status results; do
  grep -Eq "^  $field:" "$action_root/action.yml"
done

printf 'composite action self-test passed\n'
