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

`crates/web` runs `--pipeline` and `--read` in a browser page against an in-memory
filesystem, so a page can show which files a run writes. It builds only for
`wasm32-unknown-unknown`; the GUI, log files and downloads stay native-only.

```bash
rustup target add wasm32-unknown-unknown
cargo build -p mlo_merger_web --release --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir web/pkg \
  target/wasm32-unknown-unknown/release/mlo_merger_web.wasm
```

`wasm-bindgen` comes from `cargo install wasm-bindgen-cli` at the version of the
`wasm-bindgen` crate in `Cargo.lock`. The page fills the filesystem with
`web/vfs.js`'s `addFile`, calls `run(args, onLog)`, and draws `snapshot()`. `onChange` reports
each write as it happens.

Library code reaches the filesystem only through `crate::fs`, which re-exports `std::fs` and
`walkdir` natively and calls `web/vfs.js` in the browser. That means `fs::is_file(&path)`
rather than `path.is_file()`, `fs::current_dir()` and `fs::process_id()` rather than their
`std::env` and `std::process` versions, and no `std::time::SystemTime::now`. Native builds
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
