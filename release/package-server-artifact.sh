#!/usr/bin/env bash

exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/package-binary-artifact.sh" server "$@"
