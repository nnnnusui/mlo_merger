#!/usr/bin/env bash
# Builds the browser demo into web/dist, ready for any static file server or GitHub Pages:
#   scripts/build_web.sh && python3 -m http.server -d web/dist
# Needs the wasm32-unknown-unknown target and wasm-bindgen-cli at the version of the
# wasm-bindgen crate in Cargo.lock. Runs wasm-opt as well when it is installed.
set -euo pipefail
cd "$(dirname "$0")/.."

out=web/dist
cargo build --locked -p mlo_merger_web --profile web --target wasm32-unknown-unknown
rm -rf "$out"
mkdir -p "$out"
wasm-bindgen --target web --no-typescript --remove-name-section --remove-producers-section \
  --out-dir "$out/pkg" \
  target/wasm32-unknown-unknown/web/mlo_merger_web.wasm
if command -v wasm-opt >/dev/null; then
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
    -o "$out/pkg/mlo_merger_web_bg.wasm" "$out/pkg/mlo_merger_web_bg.wasm"
fi
cp web/index.html web/style.css web/main.js web/worker.js web/vfs.js "$out/"
# The sample workspace, with its own configuration; see web/sample/README.md.
cp -r web/sample/. "$out/sample/"
rm "$out/sample/README.md"
echo "Built $out"
