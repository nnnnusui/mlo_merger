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

A Windows executable can be built from Linux with
[cargo-xwin](https://github.com/rust-cross/cargo-xwin) and clang/lld. Only the MSVC target
works, since `nethost-sys` has no `windows-gnu` binaries. cargo-xwin downloads the MSVC
CRT and Windows SDK, accepting Microsoft's license:

```bash
rustup target add x86_64-pc-windows-msvc
XWIN_ACCEPT_LICENSE=1 cargo xwin build --release --target x86_64-pc-windows-msvc
```

## Tests

```bash
cargo test --lib
cargo test --doc
cargo test --test codewalker_roundtrip
```

Some tests require local game assets or CodeWalker; many are ignored by default.

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
