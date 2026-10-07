#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ ! -f web/pkg/belote_client_bg.wasm || ! -x target/release/belote-server ]]; then
  ./scripts/build.sh
fi
export BELOTE_WEB_DIR="$PWD/web"
exec ./target/release/belote-server
