# MLO Merger

[![SUSHI-WARE LICENSE](https://img.shields.io/badge/license-SUSHI--WARE%F0%9F%8D%A3-blue.svg)](https://github.com/MakeNowJust/sushi-ware)

Merges FiveM YMAP resources and conflicting YBN collision files against vanilla data.
See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the processing flow and
[docs/TODO.md](docs/TODO.md) for remaining work.

## Requirements

- Rust stable.
- A schema-1 `vanilla-cache` for pipeline merging; standalone YMAP/YBN merge commands accept their existing vanilla inputs. The default workspace is `asset`.
- GTA V Legacy and a locally built CodeWalker bridge for RPF cache generation.
- Local META schemas for XML-to-YMAP conversion. Matching binary files are discovered automatically, or supplied with `--schema-dir`.

The Native Rust backend is the default. Optional CodeWalker conversion and YBN
rebuilding require the .NET 8 SDK and a local `CodeWalker.Core.dll`; the DLL is
not distributed here. Check its license before using or redistributing it.

```bash
export CODEWALKER_CORE_DLL=/path/to/CodeWalker.Core.dll
cargo build
```

Without that variable, the build looks for `asset/CodeWalker.Core.dll`.
`CODEWALKER_BRIDGE_DLL` can override the published bridge location.

## Merge Pipeline

```bash
cargo run
cargo run -- --workspace asset --source-dir asset/source
cargo run -- --output-resource-dir asset/custom_resource
```

The pipeline reads mod files from `source` and vanilla history from
`vanilla-cache`. For each matching file it selects the lowest-difference
baseline from newest to oldest, then merges changes onto the latest cached
vanilla state. The default outputs are `asset/.output/extracted`,
`asset/.output/extracted.xml`, `asset/.output/merged.xml`,
`asset/.output/merged/ymap`, `asset/.output/merged/ybn`, and
`asset/.output/merged_mlo`. Use `--output-resource-dir` to choose another
deployment directory.

Existing generated outputs are replaced after confirmation. Deployment also
replaces the destination's generated stream contents and omit list. Use `-y`
to skip confirmation, and `--use-codewalker-dll` to select the optional backend.
Generate `asset/vanilla-cache` before running the pipeline. YMAP XML conversion
uses the matching extracted mod binary as its resource template.

## Vanilla Cache

```bash
cargo run -- --generate-vanilla-cache -i /mnt/gtav -o asset/vanilla-cache
cargo run -- --generate-vanilla-cache -i /mnt/gtav -o asset/vanilla-cache --through-version patchday1ng
cargo run -- --list-vanilla-versions example.ybn --vanilla-cache asset/vanilla-cache
```

Each run removes and recreates the selected output directory. Generation reads
base archives, the title update, and DLCs in `dlclist.xml` order without modifying
the game installation. `--through-version` accepts `base`, `update`, or a DLC
label and generates the complete history prefix through that stage, skipping
later archives. Earlier overlays are included because they determine the selected
stage's cumulative state. Version IDs are installed-file stages, not build numbers.
Each stage stores the extracted raw YMAP/YBN file when it is new or differs from
the preceding cumulative state; unchanged files are omitted. This preserves the
exact bytes from the source archive.
The cache manifest currently accepts schema 1 only. Regenerate a cache to use
this raw-per-stage layout.

Version lookup returns a JSON list of introductions and changes. Matching is
case-insensitive; unchanged stages are omitted and an unknown name returns `[]`.

## MLO Diff Cache

```bash
cargo run -- --generate-diff-cache \
  -i asset/source -o asset/source-cache --vanilla-cache asset/vanilla-cache
```

The default `generate-diff-cache` Cargo alias scans all resources in `asset/source`,
including nested bracket groups. The output directory must be empty. Use `--input`
for a single resource or another resources root.
Each resource selects a vanilla baseline from its files' closest versions, then
saves YMAP/YBN differences, selection metadata and logs. Unmatched or unsupported
files are recorded rather than compared. These reports are for inspection and
are not read by the main merge pipeline. Unsupported YBN semantic comparisons
are recorded as unsupported; the diff cache does not store full-model fallback
artifacts.

## Individual Commands

```bash
# Native file/XML conversion
cargo run -- --to-xml -i collision.ybn -o exported
cargo run -- --from-xml -i exported/collision.ybn.xml -o rebuilt
cargo run -- --from-xml -i map.ymap.xml -o rebuilt --schema-dir asset/extracted

# PSO XML requires its original binary as a template
cargo run -- --from-xml -i map.ymap.pso.xml -o rebuilt --schema-dir original/map.ymap

# Extraction and manual merging
cargo run -- --extract-ymap --flatten -i asset/source -o asset/extracted --vanilla-dir asset/vanilla/ymap.xml
cargo run -- --merge-ymap-xml --vanilla-dir asset/vanilla/ymap.xml --mod-dir asset/extracted.xml --mod-ymap-dir asset/extracted --output-dir asset/merged.xml
cargo run -- --merge-ybn --workspace asset

# Diagnostics and parent-cache preparation
cargo run -- --check-stream-conflicts -i asset/source -o asset/stream-conflicts.json
cargo run -- --build-ymap-cache --vanilla-dir asset/vanilla/ymap.xml
```

Use `--blacklist-config <FILE>` when merging to exclude configured occlude models.
Run `cargo run -- --help` for all options. Convenience commands are defined in
[.cargo/config.toml](.cargo/config.toml).

## Limitations

- Cache stages do not reproduce the complete game-engine mount rules or historical installations.
- MLO semantic differences are reports, not lossless history patches.
- Conversion support depends on the resource family and available schemas. PSO rebuilding cannot grow arrays or allocated strings; RBF YMT is unsupported.
- Matching XML or stable rebuilds does not establish in-game load safety. Validate generated resources in the target game environment.

## Tests

```bash
cargo test --lib
cargo test --doc
```

Some tests require local game assets or CodeWalker and are ignored by default.
Game assets and CodeWalker binaries are not redistributed.