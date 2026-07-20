#!/usr/bin/env bash

set -euo pipefail
umask 077

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
SERVER_IMAGE=minio/minio@sha256:14cea493d9a34af32f524e538b8346cf79f3321eff8e708c1e2960462bd8936e
CLIENT_IMAGE=minio/mc@sha256:a7fe349ef4bd8521fb8497f55c6042871b2ae640607cf99d9bede5e9bdf11727
RUN_ID="${BASHPID:-$$}"
CONTAINER="blobyard-minio-acceptance-$RUN_ID"
NETWORK="blobyard-minio-acceptance-$RUN_ID"
BUCKET=blobyard-acceptance
ACCESS_KEY=blobyardacceptance
SECRET_KEY=blobyardacceptancepassword

cleanup() {
  docker rm --force "$CONTAINER" >/dev/null 2>&1 || true
  docker network rm "$NETWORK" >/dev/null 2>&1 || true
}
trap cleanup EXIT HUP INT TERM

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'Required acceptance tool is unavailable: %s\n' "$1" >&2
    exit 1
  }
}

require_command cargo
require_command curl
require_command docker

docker info >/dev/null
docker image inspect "$SERVER_IMAGE" >/dev/null
docker image inspect "$CLIENT_IMAGE" >/dev/null
docker network create "$NETWORK" >/dev/null
docker run --detach --name "$CONTAINER" --network "$NETWORK" \
  --publish 127.0.0.1::9000 \
  --env "MINIO_ROOT_USER=$ACCESS_KEY" \
  --env "MINIO_ROOT_PASSWORD=$SECRET_KEY" \
  "$SERVER_IMAGE" server /data --console-address :9001 >/dev/null

PORT=$(docker port "$CONTAINER" 9000/tcp | awk -F: 'NR == 1 {print $NF}')
[[ $PORT =~ ^[1-9][0-9]*$ ]] || {
  printf 'MinIO did not publish a usable host port.\n' >&2
  exit 1
}
BRIDGE_ADDRESS=$(docker inspect --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$CONTAINER")
BRIDGE_ENDPOINT="http://$BRIDGE_ADDRESS:9000"
PUBLISHED_ENDPOINT="http://127.0.0.1:$PORT"

docker run --rm --network "$NETWORK" \
  --env "MC_HOST_local=http://$ACCESS_KEY:$SECRET_KEY@$CONTAINER:9000" \
  "$CLIENT_IMAGE" ready local >/dev/null

if curl --fail --silent --connect-timeout 2 --max-time 2 \
  "$BRIDGE_ENDPOINT/minio/health/ready" >/dev/null; then
  ENDPOINT=$BRIDGE_ENDPOINT
else
  ENDPOINT=$PUBLISHED_ENDPOINT
  curl --fail --silent --show-error --connect-timeout 2 --max-time 30 \
    --retry 30 --retry-delay 1 --retry-connrefused \
    "$ENDPOINT/minio/health/ready" >/dev/null
fi

docker run --rm --network "$NETWORK" \
  --env "MC_HOST_local=http://$ACCESS_KEY:$SECRET_KEY@$CONTAINER:9000" \
  "$CLIENT_IMAGE" mb "local/$BUCKET" >/dev/null

BLOBYARD_MINIO_ENDPOINT="$ENDPOINT" \
  BLOBYARD_MINIO_BUCKET="$BUCKET" \
  BLOBYARD_MINIO_ACCESS_KEY="$ACCESS_KEY" \
  BLOBYARD_MINIO_SECRET_KEY="$SECRET_KEY" \
  cargo run --quiet --locked --package blobyard-storage-s3 --example minio_acceptance \
  --manifest-path "$REPO_ROOT/Cargo.toml"

docker run --rm --network "$NETWORK" \
  --env "MC_HOST_local=http://$ACCESS_KEY:$SECRET_KEY@$CONTAINER:9000" \
  "$CLIENT_IMAGE" rb "local/$BUCKET" >/dev/null

printf 'Blob Yard S3 MinIO acceptance passed.\n'
