#!/usr/bin/env bash
# Builds the browser build.
#
#   scripts/build_web.sh                 the demo page into web/dist, for any static file server:
#                                        scripts/build_web.sh && python3 -m http.server -d web/dist
#   scripts/build_web.sh --bundle DIR    the `web/` directory of the release bundle into DIR:
#                                        pkg/, vfs.js, vfs.d.ts and sample/; see docs/web.md
#
# Needs the wasm32-unknown-unknown target, wasm-bindgen-cli at the version of the wasm-bindgen
# crate in Cargo.lock, and wasm-opt (binaryen). Without wasm-opt the demo is built unoptimized;
# the bundle fails instead. Either fails when the module exceeds the 25 MiB a file may have on
# Cloudflare Pages.
set -euo pipefail
cd "$(dirname "$0")/.."

bundle=
case "${1:-}" in
  --bundle) bundle=${2:?--bundle needs a directory} ;;
  "") ;;
  *) echo "Usage: $0 [--bundle DIR]" >&2; exit 2 ;;
esac
out=${bundle:-web/dist}
max_bytes=$((25 * 1024 * 1024))

locked=$(sed -n '/^name = "wasm-bindgen"$/{n;s/^version = "\(.*\)"$/\1/p;}' Cargo.lock)
installed=$(wasm-bindgen --version | cut -d' ' -f2)
if [[ "$installed" != "$locked" ]]; then
  echo "wasm-bindgen-cli $installed does not match the wasm-bindgen crate $locked in Cargo.lock" >&2
  exit 1
fi
if [[ -n "$bundle" ]] && ! command -v wasm-opt >/dev/null; then
  echo "The bundle needs wasm-opt (binaryen)" >&2
  exit 1
fi

# The `web` profile is the release profile optimized for size.
cargo build --locked -p mlo_merger_web --profile web --target wasm32-unknown-unknown
rm -rf "$out"
mkdir -p "$out"
# Leaves out DWARF (no --keep-debug) and writes no source maps.
wasm-bindgen --target web --remove-name-section --remove-producers-section \
  --out-dir "$out/pkg" \
  target/wasm32-unknown-unknown/web/mlo_merger_web.wasm
wasm="$out/pkg/mlo_merger_web_bg.wasm"
if command -v wasm-opt >/dev/null; then
  # The features rustc enables by default for wasm32-unknown-unknown.
  wasm-opt -Oz --strip-debug --strip-producers \
    --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
    --enable-mutable-globals --enable-multivalue --enable-reference-types \
    -o "$wasm" "$wasm"
fi
size=$(wc -c < "$wasm")
if (( size > max_bytes )); then
  echo "$wasm is $size bytes, over the $max_bytes a Cloudflare Pages file may have" >&2
  exit 1
fi
echo "$wasm: $size bytes"

cp web/vfs.js "$out/"
# The sample workspace, with its own configuration; see web/sample/README.md.
cp -r web/sample/. "$out/sample/"
if [[ -n "$bundle" ]]; then
  cp web/vfs.d.ts "$out/"
else
  cp web/index.html web/style.css web/main.js web/worker.js "$out/"
  rm "$out/sample/README.md"
fi
if find "$out" -name '*.map' -o -name '*.debug.wasm' | grep -q .; then
  echo "Debug information found in $out" >&2
  exit 1
fi
echo "Built $out"
