# MLO Merger

[![SUSHI-WARE LICENSE](https://img.shields.io/badge/license-SUSHI--WARE%F0%9F%8D%A3-blue.svg)](https://github.com/MakeNowJust/sushi-ware)

[日本語](docs/Readme.ja.md) | [Architecture](docs/ARCHITECTURE.md) | [Roadmap](docs/TODO.md)

Merges FiveM YMAP resources and conflicting YBN collision files against vanilla data.

## Usage

### Run The Pipeline

```bash
mlo_merger
mlo_merger -i resources -o merged_mlo --game-dir /mnt/gtav
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

### Run Individual Operations

```bash
mlo_merger --generate-vanilla -i /mnt/gtav -o asset/vanilla
mlo_merger --generate-vanilla-cache --vanilla asset/vanilla -o asset/vanilla-cache
mlo_merger --generate-source-cache -i asset/source --vanilla-cache asset/vanilla-cache -o asset/source-cache
mlo_merger --generate-source-cache resourceName -i asset/source -f
mlo_merger --merge --vanilla-cache asset/vanilla-cache --source-cache asset/source-cache -o asset/merged
mlo_merger --deploy -i asset/merged --source-cache asset/source-cache -o asset/merged_mlo
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

Run `mlo_merger --help` for options and per-command output defaults. To extract
a limited vanilla archive, use the explicit generator's `--gamebuild` stage
selector; numeric GTA build-ID mapping is not implemented. Convenience aliases
are in [.cargo/config.toml](.cargo/config.toml).

### Matching Configuration

Merge and diff-cache operations read [asset/config.toml](asset/config.toml) once
per operation. Missing files or omitted keys use these defaults:

```toml
[tolerance]
ybn = 0.05
ymap = 0.001
ymap_occlude_model = 0.01
ymap_box_occluder = 1
```

`ybn` controls inclusive per-axis polygon coordinates and radius matching across
all supported polygon types, not only boxes. `ymap` controls floating scalar
comparisons for car generators and LOD lights. Occlude models use their XY extent
tolerance; box occluders use integer stored-coordinate units. YMAP comparisons
remain exclusive of the boundary. Entity coordinates remain exact, and existing
position-key rounding is unchanged. These settings do not change search radii.

All values must be positive; floating values must also be finite. Unknown keys
and invalid TOML are errors. Effective settings are included in merge freshness,
so changing them causes regeneration on the next merge without requiring `-f`.
Larger tolerances can treat intentionally different nearby shapes as identical.

### Inspect Existing Data

```bash
mlo_merger --find entity-guid 2443198849 --filter '*cs4_10_strm_0.ymap' --diff-all
mlo_merger --find entity-position 3.5,4.2,0.0 --round 1.0
mlo_merger --find ybn-position 1211.126,-507.5948,67.54723 --radius 1.0 --type box --filter id2_21_c_0.ybn --diff-all
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

### Convert Files

```bash
mlo_merger --to-xml collision.ybn -o exported
mlo_merger --from-xml exported/collision.ybn.xml -o rebuilt
mlo_merger --from-xml map.ymap.xml -o rebuilt --vanilla asset/vanilla
```

Inputs can be files or directories. Conversion output defaults to the current
directory. META resources need compatible schemas; `--vanilla` supplies schema
or template inputs. YBN conversion does not require an external schema directory.

### TypeScript Types

The release archives contain `asset/gen_mlo_merger.ts`, generated from the Rust
Serde models for merge diagnostics, vanilla/source/merge/deploy caches, stream
conflicts, diff-cache reports, version listings and matching/blacklist
configuration. It does not include YMAP/YBN difference payloads or JSON Schema
documents. The release workflow regenerates it before packaging; it is not
checked into the repository.

When generating it locally, import it from its output path:

```typescript
import type { DuplicateReport, MatchingConfig } from "./asset/gen_mlo_merger";

const config: MatchingConfig = { tolerance: { ybn: 0.05 } };
```

Types preserve JSON field names, nullability and optional inputs. Paths are
strings; sets are arrays. File sizes and timestamps are JSON numbers, represented
as TypeScript `number`, not `bigint`. Values above `Number.MAX_SAFE_INTEGER` need
a precision-aware JSON reader. These definitions do not validate data at runtime.

### Limitations

- `--get-diff` currently prints a mock invocation and does not compare files.
- Merge uses the latest selected vanilla, without historical baseline inference.
- Cache stages do not reproduce the complete game-engine mount rules or historical installations.
- MLO semantic differences are reports, not lossless history patches.
- Conversion support depends on the resource family and available schemas. PSO rebuilding cannot grow arrays or allocated strings; RBF YMT is unsupported.
- Matching XML or stable rebuilds does not establish in-game load safety. Validate generated resources in the target game environment.

## Development Environment

- Rust stable on Windows or Linux.
- An installed GTA V and locally supplied CodeWalker bridge/assemblies are needed
	to extract from game archives. An existing vanilla archive can be reused.

Build the executable with:

```bash
cargo build
```

The examples above use `mlo_merger` as the executable name. If it is not on your
`PATH`, invoke it using its path (for example, `./target/debug/mlo_merger`).

### Generate TypeScript Types

TypeScript definitions are generated from Rust's Serde models. The optional
`typescript` feature is required only for generation and verification, not for
normal builds.

```bash
cargo export-types
cargo export-types -o /tmp/mlo_merger.ts
cargo export-types --output /tmp/mlo_merger.ts
cargo test --features typescript --lib typescript::tests
```

### Run Tests

```bash
cargo test --lib
cargo test --doc
cargo test --test codewalker_roundtrip
```

Some tests require local game assets or CodeWalker; many are ignored by default.
Game assets and CodeWalker binaries are not redistributed.