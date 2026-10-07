#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
rustup target add wasm32-unknown-unknown
if [[ ! -x .tools/wasm-bindgen ]]; then
  cargo install wasm-bindgen-cli --version 0.2.104 --locked --root .tools
  cp .tools/bin/wasm-bindgen .tools/wasm-bindgen
fi
cargo build --locked -p belote-client --target wasm32-unknown-unknown --release
.tools/wasm-bindgen target/wasm32-unknown-unknown/release/belote_client.wasm --target web --out-dir web/pkg --no-typescript
cargo build --locked -p belote-server --release
printf '\nReady. Start the server with ./scripts/run.sh\n'
