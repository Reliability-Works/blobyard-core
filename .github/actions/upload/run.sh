#!/usr/bin/env bash

set -euo pipefail

fail() {
  printf 'Blobyard action: %s\n' "$1" >&2
  exit 1
}

require() {
  command -v "$1" >/dev/null 2>&1 || fail "required tool is unavailable: $1"
}

boolean() {
  case $1 in
    true | false) printf '%s\n' "$1" ;;
    *) fail "$2 must be true or false" ;;
  esac
}

operation() {
  case $1 in
    upload | deploy) printf '%s\n' "$1" ;;
    *) fail 'operation must be upload or deploy' ;;
  esac
}

validate_api_url() {
  if [[ $1 =~ ^https://[^/?#[:space:]]+/v1$ || $1 =~ ^http://(localhost|127\.0\.0\.1)(:[0-9]+)?/v1$ ]]; then
    return
  fi
  fail 'api-url must be a versioned HTTPS /v1 base, except for a loopback development base'
}

validate_web_yard_origin() {
  local authority port
  if [[ $1 =~ ^https://[a-z0-9]([a-z0-9.-]*[a-z0-9])?(:[0-9]+)?$ ]] ||
    [[ $1 =~ ^http://localhost(:[0-9]+)?$ ]]; then
    authority=${1#*://}
    if [[ $authority == *:* ]]; then
      port=${authority##*:}
      ((port >= 1 && port <= 65535)) || fail 'web-yard-origin port must be between 1 and 65535'
    fi
    return
  fi
  fail 'web-yard-origin must be an HTTPS domain root, or HTTP on localhost for development'
}

install_cli() {
  if [[ -n $BLOBYARD_ACTION_LOCAL_CLI ]]; then
    [[ -x $BLOBYARD_ACTION_LOCAL_CLI ]] || fail 'local-cli-path is not executable'
    printf '%s\n' "$BLOBYARD_ACTION_LOCAL_CLI"
    return
  fi

  local root install_directory
  root=$(cd "$GITHUB_ACTION_PATH/../../.." && pwd)
  install_directory=${RUNNER_TEMP:?RUNNER_TEMP is required}/blobyard-action/bin
  "$root/scripts/install.sh" --install-dir "$install_directory" >&2
  printf '%s\n' "$install_directory/blobyard"
}

request_oidc_token() {
  [[ -n ${ACTIONS_ID_TOKEN_REQUEST_URL:-} && -n ${ACTIONS_ID_TOKEN_REQUEST_TOKEN:-} ]] ||
    fail 'no token supplied and GitHub OIDC is unavailable; add permissions: id-token: write'

  local audience response oidc
  audience=${BLOBYARD_ACTION_API_URL%/v1}
  response=$(curl --fail --silent --show-error --connect-timeout 10 --max-time 30 \
    -H "Authorization: bearer $ACTIONS_ID_TOKEN_REQUEST_TOKEN" \
    --get --data-urlencode "audience=$audience" \
    "$ACTIONS_ID_TOKEN_REQUEST_URL") || fail 'GitHub OIDC token request failed'
  oidc=$(jq -er '.value | select(type == "string" and length > 0)' <<<"$response") || fail 'GitHub OIDC response did not include a token'
  [[ $oidc != *$'\n'* && $oidc != *$'\r'* ]] || fail 'GitHub OIDC response contained an invalid token'
  printf '%s\n' "$oidc"
}

exchange_oidc_token() {
  local oidc=$1 body response status token actions
  if [[ $action_operation == deploy ]]; then
    actions='["yard:manage","upload"]'
  elif [[ $share == true ]]; then
    actions='["upload","share"]'
  else
    actions='["upload"]'
  fi
  body=$(jq -cn \
    --arg workspace "$BLOBYARD_ACTION_WORKSPACE" \
    --arg project "$BLOBYARD_ACTION_PROJECT" \
    --argjson actions "$actions" \
    '{workspace:$workspace,project:$project,actions:$actions}')
  response=$(mktemp)
  trap 'rm -f "$response"' RETURN
  status=$(curl --silent --show-error --connect-timeout 10 --max-time 30 --output "$response" --write-out '%{http_code}' \
    -H "Authorization: Bearer $oidc" -H 'Content-Type: application/json' \
    --data "$body" "$BLOBYARD_ACTION_API_URL/ci/github/oidc/exchange") || fail 'Blobyard OIDC exchange request failed'
  [[ $status == 200 ]] || fail "Blobyard OIDC exchange failed with HTTP $status"
  token=$(jq -er '
    select(.ok == true) |
    .data |
    select(
      (.accessToken | type == "string" and length > 0) and
      (.expiresInSeconds | type == "number" and floor == . and . > 0) and
      (.scopes | type == "array" and length > 0 and all(.[]; type == "string" and length > 0))
    ) |
    .accessToken
  ' "$response") || fail 'Blobyard OIDC exchange returned an invalid envelope'
  [[ $token != *$'\n'* && $token != *$'\r'* ]] || fail 'Blobyard OIDC exchange returned an invalid token'
  printf '%s\n' "$token"
}

run_cli_json() {
  local output=$1
  shift
  if ! "$cli" --json --api-url "$BLOBYARD_ACTION_API_URL" \
    --web-yard-origin "$BLOBYARD_ACTION_WEB_YARD_ORIGIN" "${scope[@]}" "$@" >"$output"; then
    cat "$output" >&2
    fail 'Blobyard CLI command failed'
  fi
  jq -e 'select(.ok == true) and (.data | type == "object")' "$output" >/dev/null || fail 'Blobyard CLI returned an invalid JSON envelope'
}

run_deploy_json() {
  local output=$1 allow_partial=$2 status
  shift 2
  if "$cli" --json --api-url "$BLOBYARD_ACTION_API_URL" \
    --web-yard-origin "$BLOBYARD_ACTION_WEB_YARD_ORIGIN" "${scope[@]}" "$@" >"$output"; then
    status=0
  else
    status=$?
  fi
  if [[ $status == 0 ]] && jq -e 'select(.ok == true) and (.data | type == "object")' "$output" >/dev/null; then
    return
  fi
  if [[ $allow_partial == true && $status != 0 ]] &&
    jq -e 'select(.ok == false) and (.data.results | type == "array") and (.error | type == "object")' "$output" >/dev/null; then
    return
  fi
  cat "$output" >&2
  fail 'Blobyard CLI deploy command failed'
}

validate_machine_identity() {
  local output=$1
  run_cli_json "$output" whoami
  jq -e --arg operation "$action_operation" --argjson share "$share" '
    .data.principalType == "ci" and
    (.data.scopes | type == "array") and
    (if $operation == "deploy" then
      (.data.scopes | index("yard:manage") != null) and
      (.data.scopes | index("upload") != null)
    else
      (.data.scopes | index("upload") != null) and
      (($share | not) or (.data.scopes | index("share") != null))
    end) and
    (.data.defaultWorkspace.slug | type == "string" and length > 0)
  ' "$output" >/dev/null || fail 'token must be a scoped Blobyard CI machine token'
  if [[ -n ${BLOBYARD_ACTION_WORKSPACE:-} ]]; then
    jq -e --arg workspace "$BLOBYARD_ACTION_WORKSPACE" \
      '.data.defaultWorkspace.slug == $workspace' "$output" >/dev/null ||
      fail 'token is not scoped to the requested workspace'
  fi
}

single_upload_result() {
  local output=$1 count
  count=$(jq -er '.data.files | select(type == "array") | length' "$output") ||
    fail 'upload response omitted files'
  [[ $count == 1 ]] ||
    fail 'the action requires path to resolve to exactly one uploaded file for its singular outputs'
  jq -er '.data.files[0].uri | select(type == "string" and length > 0)' "$output" ||
    fail 'upload response omitted uri'
}

uri_version() {
  local uri=$1 version
  [[ $uri == blobyard://* ]] || fail 'upload response returned a non-Blobyard URI'
  if [[ $uri =~ [?\&]version=([1-9][0-9]*)($|\&) ]]; then
    version=${BASH_REMATCH[1]}
  else
    fail 'upload response returned a URI without an immutable version'
  fi
  printf '%s\n' "$version"
}

deploy_results() {
  local output=$1 origin=$2
  jq -ce --arg origin "$origin" '
    def trusted_web_yard_url($trusted_origin):
      ($trusted_origin | capture("^(?<scheme>https?)://(?<authority>[^/?#]+)$")) as $trusted |
      ($trusted.scheme + "://") as $prefix |
      ("." + $trusted.authority) as $suffix |
      . as $url |
      ($url | startswith($prefix)) and
      ($url | endswith($suffix)) and
      (($url | ltrimstr($prefix) | rtrimstr($suffix)) |
        test("^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$"));
    if (.data.results | type) == "array" then .data.results
    else [{
      yard: .data.yard, ok: true, yardUrl: .data.yardUrl,
      deploymentUrl: .data.deploymentUrl, deployId: .data.deployId, status: .data.status
    }]
    end |
    select(length > 0) |
    select(all(.[];
      (.yard | type == "string" and length > 0) and
      (.ok | type == "boolean") and
      (.status | IN("uploading", "finalising", "live", "failed", "superseded", "pruned")) and
      (if .ok then
        (.yardUrl | type == "string" and trusted_web_yard_url($origin)) and
        (.deploymentUrl | type == "string" and trusted_web_yard_url($origin)) and
        (.deployId | type == "string" and length > 0)
      else
        (.error.code | type == "string" and length > 0) and
        (.error.message | type == "string" and length > 0)
      end)
    ))
  ' "$output" || fail 'deploy response omitted valid Web Yard results'
}

single_deploy_value() {
  local output=$1 field=$2
  jq -er --arg field "$field" '.data[$field] | select(type == "string" and length > 0)' "$output" ||
    fail "deploy response omitted $field"
}

write_output() {
  local name=$1 value=$2
  [[ $value != *$'\n'* && $value != *$'\r'* ]] || fail "CLI output $name contains a newline"
  printf '%s=%s\n' "$name" "$value" >>"${GITHUB_OUTPUT:?GITHUB_OUTPUT is required}"
}

validate_output_value() {
  local name=$1 value=$2
  [[ -n $value ]] || fail "CLI output $name is empty"
  [[ $value != *$'\n'* && $value != *$'\r'* ]] || fail "CLI output $name contains a newline"
}

comment_on_pr() {
  local uri=$1 share_url=$2 yard_url=$3 deployment_url=$4 deploy_id=$5 deploy_status=$6 results=$7
  local number body actor comment_id comments marker rendered
  require gh
  [[ -n ${GITHUB_REPOSITORY:-} ]] || fail 'comment-on-pr requires GITHUB_REPOSITORY'
  number=${BLOBYARD_ACTION_PR_NUMBER:-}
  if [[ -n $number ]]; then
    [[ $number =~ ^[1-9][0-9]*$ ]] || fail 'pull-request-number must be a positive integer'
  else
    [[ -n ${GITHUB_EVENT_PATH:-} && -f $GITHUB_EVENT_PATH ]] ||
      fail 'comment-on-pr requires a pull_request event or pull-request-number'
    number=$(jq -er '.pull_request.number | select(type == "number" and floor == . and . > 0)' \
      "$GITHUB_EVENT_PATH") || fail 'comment-on-pr requires a pull_request event or pull-request-number'
  fi
  gh api "repos/$GITHUB_REPOSITORY/pulls/$number" --jq .number >/dev/null ||
    fail 'pull-request-number did not identify an accessible pull request'
  marker='<!-- blobyard-upload -->'
  if [[ $action_operation == upload ]]; then
    body="$marker
Blobyard upload: \`$uri\`"
    [[ -n $share_url ]] && body="$body

Share: $share_url"
  elif [[ -n $yard_url ]]; then
    body="$marker
Blobyard Web Yard: $yard_url

Immutable deployment: $deployment_url

Deploy: \`$deploy_id\`
Status: $deploy_status"
  else
    rendered=$(jq -r '.[] | if .ok then
      "- \(.yard): \(.yardUrl) (\(.status))\n  Immutable deployment: \(.deploymentUrl)"
    else
      "- \(.yard): failed [\(.error.code)] \(.error.message)"
    end' <<<"$results") ||
      fail 'Web Yard comment results were invalid'
    body="$marker
Blobyard Web Yard deploys:

$rendered"
  fi
  actor=$(gh api user --jq .login) || fail 'pull request comment identity lookup failed'
  comments=$(mktemp)
  trap 'rm -f "$comments"' RETURN
  gh api --paginate --slurp \
    "repos/$GITHUB_REPOSITORY/issues/$number/comments" >"$comments" ||
    fail 'pull request comment lookup failed'
  comment_id=$(jq -r --arg actor "$actor" --arg marker "$marker" '
    [ .[][] | select(.user.login == $actor and (.body | startswith($marker))) ][0].id // empty
  ' "$comments") || fail 'pull request comment lookup returned invalid data'
  if [[ -n $comment_id ]]; then
    gh api "repos/$GITHUB_REPOSITORY/issues/comments/$comment_id" \
      --method PATCH --field body="$body" >/dev/null || fail 'pull request comment update failed'
  else
    gh api "repos/$GITHUB_REPOSITORY/issues/$number/comments" \
      --method POST --field body="$body" >/dev/null || fail 'pull request comment creation failed'
  fi
}

require curl
require jq
validate_api_url "${BLOBYARD_ACTION_API_URL:?api-url is required}"
validate_web_yard_origin "${BLOBYARD_ACTION_WEB_YARD_ORIGIN:?web-yard-origin is required}"
[[ -e ${BLOBYARD_ACTION_PATH:?path is required} ]] || fail 'path does not exist'
[[ -n ${BLOBYARD_ACTION_PROJECT:?project is required} ]] || fail 'project is empty'
action_operation=$(operation "${BLOBYARD_ACTION_OPERATION:-upload}")
share=$(boolean "${BLOBYARD_ACTION_SHARE:-false}" share)
spa=$(boolean "${BLOBYARD_ACTION_SPA:-false}" spa)
clean_urls=$(boolean "${BLOBYARD_ACTION_CLEAN_URLS:-false}" clean-urls)
public=$(boolean "${BLOBYARD_ACTION_PUBLIC:-false}" public)
comment=$(boolean "${BLOBYARD_ACTION_COMMENT:-false}" comment-on-pr)
yard=${BLOBYARD_ACTION_YARD:-}
if [[ $action_operation == deploy ]]; then
  [[ $share == false ]] || fail 'share is available only for upload operations'
  [[ $public == true ]] || fail 'public must be true for Web Yard deploys'
  [[ -d $BLOBYARD_ACTION_PATH ]] || fail 'path must be a directory for Web Yard deploys'
elif [[ -n $yard || $spa == true || $clean_urls == true || $public == true ]]; then
  fail 'yard, spa, clean-urls, and public are available only for deploy operations'
fi
github_token=${BLOBYARD_ACTION_GITHUB_TOKEN:-}
token=${BLOBYARD_ACTION_TOKEN:-}
unset BLOBYARD_ACTION_GITHUB_TOKEN BLOBYARD_ACTION_TOKEN
if [[ -n $token ]]; then
  [[ $token != *$'\n'* && $token != *$'\r'* ]] || fail 'explicit token contains a newline'
fi
if [[ $comment == true ]]; then
  [[ -n $github_token ]] || fail 'comment-on-pr requires github-token or github.token'
  [[ $github_token != *$'\n'* && $github_token != *$'\r'* ]] ||
    fail 'GitHub token contains a newline'
fi
cli=$(install_cli)
[[ -n $token ]] || token=$(exchange_oidc_token "$(request_oidc_token)")
export BLOBYARD_TOKEN=$token

temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
validate_machine_identity "$temporary/whoami.json"
workspace=$(jq -er '.data.defaultWorkspace.slug' "$temporary/whoami.json") ||
  fail 'machine identity omitted its workspace scope'
scope=(--project "$BLOBYARD_ACTION_PROJECT" --workspace "$workspace")
uri=
object_version=
share_url=
yard_url=
deployment_url=
deploy_id=
deploy_status=
results='[]'
deploy_failed=false

if [[ $action_operation == upload ]]; then
  run_cli_json "$temporary/upload.json" upload -- "$BLOBYARD_ACTION_PATH"
  uri=$(single_upload_result "$temporary/upload.json")
  object_version=$(uri_version "$uri")
  if [[ $share == true ]]; then
    run_cli_json "$temporary/share.json" share "$uri" --expires "${BLOBYARD_ACTION_EXPIRES:-7d}"
    share_url=$(jq -er '(.data.shareUrl // .data.url) | select(type == "string" and length > 0)' "$temporary/share.json") || fail 'share response omitted share URL'
  fi
else
  deploy_args=(deploy)
  [[ $spa == false ]] || deploy_args+=(--spa)
  [[ $clean_urls == false ]] || deploy_args+=(--clean-urls)
  deploy_args+=(--public)
  if [[ -n $yard ]]; then
    deploy_args+=(--yard "$yard" -- "$BLOBYARD_ACTION_PATH")
    run_deploy_json "$temporary/deploy.json" false "${deploy_args[@]}"
  else
    deploy_args+=(--all)
    (cd "$BLOBYARD_ACTION_PATH" && run_deploy_json "$temporary/deploy.json" true "${deploy_args[@]}")
  fi
  results=$(deploy_results "$temporary/deploy.json" "$BLOBYARD_ACTION_WEB_YARD_ORIGIN")
  [[ $(jq -r '.ok' "$temporary/deploy.json") == true ]] || deploy_failed=true
  if [[ -n $yard ]]; then
    yard_url=$(single_deploy_value "$temporary/deploy.json" yardUrl)
    deployment_url=$(single_deploy_value "$temporary/deploy.json" deploymentUrl)
    deploy_id=$(single_deploy_value "$temporary/deploy.json" deployId)
    deploy_status=$(single_deploy_value "$temporary/deploy.json" status)
  fi
fi

[[ -z $uri ]] || validate_output_value uri "$uri"
[[ -z $object_version ]] || validate_output_value version "$object_version"
[[ -z $share_url ]] || validate_output_value share-url "$share_url"
[[ -z $share_url || $share_url == https://* ]] || fail 'share response returned a non-HTTPS URL'
[[ -z $yard_url ]] || validate_output_value yard-url "$yard_url"
[[ -z $deployment_url ]] || validate_output_value deployment-url "$deployment_url"
[[ -z $deploy_id ]] || validate_output_value deploy-id "$deploy_id"
[[ -z $deploy_status ]] || validate_output_value status "$deploy_status"
validate_output_value results "$results"
unset BLOBYARD_TOKEN token
if [[ $comment == true ]]; then
  export GH_TOKEN=$github_token
  comment_on_pr "$uri" "$share_url" "$yard_url" "$deployment_url" "$deploy_id" "$deploy_status" "$results"
  unset GH_TOKEN
fi
unset github_token
write_output uri "$uri"
write_output version "$object_version"
write_output share-url "$share_url"
write_output yard-url "$yard_url"
write_output deployment-url "$deployment_url"
write_output deploy-id "$deploy_id"
write_output status "$deploy_status"
write_output results "$results"
[[ $deploy_failed == false ]] || fail 'one or more Web Yard deploys failed; inspect the results output'
