# MLO Merger

[![SUSHI-WARE LICENSE](https://img.shields.io/badge/license-SUSHI--WARE%F0%9F%8D%A3-blue.svg)](https://github.com/MakeNowJust/sushi-ware)

[日本語](docs/Readme.ja.md) | [Architecture](docs/ARCHITECTURE.md) | [Roadmap](docs/TODO.md)

Merges FiveM YMAP resources and conflicting YBN collision files against vanilla data.

## Requirements

- Rust stable on Windows or Linux.
- An installed GTA V and locally supplied CodeWalker bridge/assemblies for
	extraction from game archives. An existing vanilla archive can be reused.

```bash
cargo build
```

## Run The Pipeline

```bash
cargo run
cargo run -- -i resources -o merged_mlo --game-dir /mnt/gtav
```

The default operation extracts vanilla only when its archive is missing, updates
the vanilla and source caches, merges, then deploys. Unchanged stages are reused.
`--game-dir` defaults to `/mnt/gtav` and is used only for missing vanilla data.

| Path | Default | Option |
|---|---|---|
| Source resources | `asset/source` | `-i` |
| Raw vanilla archive | `asset/vanilla` | `--vanilla` |
| Derived vanilla cache | `asset/vanilla-cache` | `--vanilla-cache` |
| Source cache | `asset/source-cache` | `--source-cache` |
| Merged files | `asset/merged` | `--merged` |
| Deployment | `asset/merged_mlo` | `-o` |

**Source-cache generation moves vanilla-named YMAP/YBN files out of resource
stream directories.** They remain available in the source cache; unmatched files
stay in the original resources. Resource names must be unique across groups.
Overlapping input and artifact directories are rejected.

Pipeline `-f` forces the derived caches, merge and deployment, but reuses an
existing raw vanilla archive. Pipeline-wide `--gamebuild` and `--step-name`
selection are not implemented.

## Run Individual Operations

```bash
cargo run -- --generate-vanilla -i /mnt/gtav -o asset/vanilla
cargo run -- --generate-vanilla-cache --vanilla asset/vanilla -o asset/vanilla-cache
cargo run -- --generate-source-cache -i asset/source --vanilla-cache asset/vanilla-cache -o asset/source-cache
cargo run -- --generate-source-cache resourceName -i asset/source -f
cargo run -- --merge --vanilla-cache asset/vanilla-cache --source-cache asset/source-cache -o asset/merged
cargo run -- --deploy -i asset/merged --source-cache asset/source-cache -o asset/merged_mlo
```

Source-cache selection accepts a resource name or input-relative resource path;
`-f` forces that selection, not its prerequisites. Replaced cached files are
retained under `_old/`. Files are cached at
`resources/{resourceName}/{stream-relative-path}`, without the leading
`stream/` or `streams/` component.

Merge writes native files under `ymap/` and `ybn/`, plus `_omit.txt` for replaced
source paths and `duplicates.json` for competing YMAP entity changes. It rebuilds
only changed or invalid groups. Failed merges leave the prior output intact.

Deploy consumes existing merge and source-cache data without regenerating them:

- Merged files: `stream/{extension}/merged/{filename}`.
- Remaining cached files: `stream/{extension}/clone/{resourceName}/{stream-relative-path}`.
- `files.txt`: original source-relative paths of active files moved into the cache.

Deploy skips unchanged copies, repairs missing/changed outputs, and removes stale
managed files. Unrelated files are preserved; locally modified stale files are
rejected rather than deleted. Empty managed stream directories are pruned.

Run `cargo run -- --help` for options and per-command output defaults. To extract
a limited vanilla archive, use the explicit generator's `--gamebuild` stage
selector; numeric GTA build-ID mapping is not implemented. Convenience aliases
are in [.cargo/config.toml](.cargo/config.toml).

## Inspect Existing Data

```bash
cargo run -- --find entity-guid 2443198849 --filter '*cs4_10_strm_0.ymap' --diff-all
cargo run -- --find entity-position 3.5,4.2,0.0 --round 1.0
cargo run -- --find ybn-position 1211.126,-507.5948,67.54723 --radius 1.0 --type box --filter id2_21_c_0.ybn --diff-all
```

Searches write XML to stdout without running the pipeline or modifying artifacts.
`-i` selects the merged directory. `--filter` is a case-insensitive filename glob;
quote wildcard patterns. Duplicate occurrences are retained.

Position searches include the 3D radius boundary and default to `1.0`. Negative
coordinates are supported; non-finite/out-of-range coordinates and negative or
non-finite radii are rejected. For entities, radius `0` selects the exact
native-precision position. YBN searches match world-space shape centers, not any point inside the
shape. `--type` accepts `box`, `triangle`, `sphere`, `capsule` or `cylinder`;
omitting it selects all supported shapes. Unsupported shapes are skipped.

`--diff-all` includes recorded vanilla/source inputs and identifies their resource
and path. `found="false"` marks a stage without a match. Changed input fingerprints
cause an error requiring a fresh merge. This is a snapshot comparison, not a
computed semantic diff. YBN indices are input-local and may change on rebuild.

## Convert Files

```bash
cargo run -- --to-xml collision.ybn -o exported
cargo run -- --from-xml exported/collision.ybn.xml -o rebuilt
cargo run -- --from-xml map.ymap.xml -o rebuilt --vanilla asset/vanilla
```

Inputs can be files or directories. Conversion output defaults to the current
directory. META resources need compatible schemas; `--vanilla` supplies schema
or template inputs. YBN conversion does not require an external schema directory.

## Limitations

- `--get-diff` currently prints a mock invocation and does not compare files.
- Merge uses the latest selected vanilla, without historical baseline inference.
- Cache stages do not reproduce the complete game-engine mount rules or historical installations.
- MLO semantic differences are reports, not lossless history patches.
- Conversion support depends on the resource family and available schemas. PSO rebuilding cannot grow arrays or allocated strings; RBF YMT is unsupported.
- Matching XML or stable rebuilds does not establish in-game load safety. Validate generated resources in the target game environment.

## Tests

```bash
cargo test --lib
cargo test --doc
cargo test --test codewalker_roundtrip
```

Some tests require local game assets or CodeWalker; many are ignored by default.
Game assets and CodeWalker binaries are not redistributed.