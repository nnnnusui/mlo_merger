# Development

## Environment

- Rust stable on Windows or Linux. The repository's dev container runs Ubuntu.
- An installed GTA V Legacy and `magic.dat` are needed to extract from game archives. An
  existing vanilla archive can be reused.
- Game assets and CodeWalker binaries are not redistributed.

### Vanilla Extraction

Game archives are read natively, without CodeWalker. Their tables of contents are
encrypted: the AES key is located in the installed `GTA5.exe`, and decodes CodeWalker's
`magic.dat`, which holds the remaining NG keys and tables. Neither is redistributed. Copy
`CodeWalker.Core/Resources/magic.dat` from the
[CodeWalker](https://github.com/dexyfex/CodeWalker) sources to `./magic.dat`, or pass its
path with `--magic`. The GUI can download it from the same place.

## Build

```bash
cargo build
```

The documentation uses `mlo_merger` as the executable name. If it is not on your `PATH`,
invoke it by its path (for example, `./target/debug/mlo_merger`).

A Windows executable can be built from Linux with MinGW:

```bash
sudo apt-get install gcc-mingw-w64-x86-64
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

The release workflow builds the MSVC target instead, which
[cargo-xwin](https://github.com/rust-cross/cargo-xwin) and clang/lld can also build from
Linux; it downloads the MSVC CRT and Windows SDK, accepting Microsoft's license:

```bash
rustup target add x86_64-pc-windows-msvc
XWIN_ACCEPT_LICENSE=1 cargo xwin build --release --target x86_64-pc-windows-msvc
```

## Tests

```bash
cargo test --lib
cargo test --doc
cargo test --features codewalker --test codewalker_roundtrip -- --ignored
```

Some tests require local game assets; many are ignored by default. The CodeWalker
round-trip tests compare conversion with CodeWalker hosted in-process, which needs the
`codewalker` feature, the .NET SDK and a locally supplied `CodeWalker.Core.dll`
(`CODEWALKER_CORE_DLL`, default `asset/CodeWalker.Core.dll`), from which `build.rs` builds
`bridge/CodeWalker.Bridge`.

## Browser Build

`web/` is a static page that runs `--pipeline` and `--read` on the `web/sample` workspace in
the browser and draws the files each run adds, modifies and removes. `crates/web` builds the
merger for `wasm32-unknown-unknown` against an in-memory filesystem; the GUI, log files and
downloads stay native-only.

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked wasm-bindgen-cli --version <wasm-bindgen version in Cargo.lock>
scripts/build_web.sh
python3 -m http.server -d web/dist
```

`scripts/build_web.sh` assembles `web/dist` from the page, the WebAssembly module and the
samples; publish that directory as it is. The page runs the merger in `web/worker.js`, which
seeds `/work` with the files `web/sample/files.json` lists through `web/vfs.js`'s `addFile`,
calls `run(args, onLog)`, and sends back `snapshot()` and each write reported by `onChange`.

`web/sample` is an original vanilla archive and source resources, with every kind of YMAP item,
LOD parents, DLC generations, game-build stages and collisions; see its `README.md`. The test
module `src/core/web_sample` builds it and runs the pipeline on it; after changing either,
regenerate the files:

```bash
MLO_WRITE_SAMPLES=1 cargo test --lib web_sample
```

Library code reaches the operating system only through `crate::platform` (`src/platform`),
whose modules use `std`, `walkdir` and `indicatif` natively and the page in the browser. That
means `platform::fs` rather than `std::fs`, `fs::is_file(&path)` rather than `path.is_file()`,
`platform::process::current_dir()` and `process::id()` rather than their `std::env` and
`std::process` versions, `platform::progress` for progress bars, and no
`std::time::SystemTime::now`. The browser has no threads either: use rayon, which then runs on
the calling thread, rather than `std::thread`. Native builds
accept the `std` calls, which then fail only in the browser, so check the browser build:

```bash
CLIPPY_CONF_DIR=web cargo clippy -p mlo_merger --lib --target wasm32-unknown-unknown \
  -- -D clippy::disallowed_methods -D clippy::disallowed_types
```

Tests and native-only code (`src/main.rs`, `src/gui`, `build.rs`) may use `std` directly.

## TypeScript Types

TypeScript definitions are generated from Rust's Serde models. The optional `typescript`
feature is required only for generation and verification, not for normal builds. The
release workflow regenerates `asset/gen_mlo_merger.ts` before packaging.

```bash
cargo export-types
cargo export-types -o /tmp/mlo_merger.ts
cargo test --features typescript --lib typescript::tests
```

## Third-Party Notices

Every release archive, including `mlo_merger-all`, contains `third_party_notices.json`
next to `LICENSE.md` and `release.json`. It lists the crates compiled into the released
executables for any release target (the normal dependency graph; no build or dev
dependencies, and not this workspace's own crates), with their license texts.
`THIRD_PARTY_NOTICES.md` renders the same data for reading. CodeWalker is not shipped and
not listed.

```json
{
  "parts": ["mlo_merger"],
  "notices": [
    {
      "license": "MIT",
      "text": "Copyright (c) ...\n\nPermission is hereby granted, ...",
      "packages": [{ "name": "serde", "version": "1.0.229", "repository": "https://github.com/serde-rs/serde" }]
    }
  ]
}
```

- Each notice holds one distinct license text (CRLF normalized to LF, trimmed), with
  copyright lines included, and every package distributed under exactly that text,
  sorted by name. Notices are sorted by their first package's name.
- `license` is the SPDX identifier of the text; MIT is preferred for dual-licensed crates.
- `repository` falls back to the crate's homepage, then its crates.io page.

Other projects copy this file, so keep its format stable. The release workflow generates
it with [cargo-about](https://github.com/EmbarkStudios/cargo-about) (accepted licenses in
`about.toml`) and `scripts/third_party_notices.py`. To generate it locally:

```bash
cargo install --locked cargo-about --version 0.9.3 --features cli
for target in x86_64-pc-windows-msvc x86_64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin; do
  cargo about generate --locked --fail --format json --target "$target" -o "target/about-$target.json"
done
python3 scripts/third_party_notices.py target/about-*.json \
  -o target/third_party_notices.json --markdown target/THIRD_PARTY_NOTICES.md
```

cargo-about runs once per target because, given several targets at once, it also lists
crates that no single target compiles.
