# MLO Merger

[![SUSHI-WARE LICENSE](https://img.shields.io/badge/license-SUSHI--WARE%F0%9F%8D%A3-blue.svg)](https://github.com/MakeNowJust/sushi-ware)

[Architecture](docs/ARCHITECTURE.md) | [Roadmap](docs/TODO.md)

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
Overlapping artifact and overwrite directories are rejected. The source root
can contain them: for example, use `resources` as source while running the tool
from `resources/mlo_merged`.

Pipeline `-f` forces the derived caches, merge and deployment, but reuses an
existing raw vanilla archive. Pipeline-wide `--gamebuild` and `--step-name`
selection are not implemented.

### Run Individual Operations

```bash
mlo_merger --generate-vanilla -i /mnt/gtav -o asset/vanilla
mlo_merger --generate-vanilla-cache --vanilla asset/vanilla -o asset/vanilla-cache
mlo_merger --generate-source-cache -i asset/source --vanilla-cache asset/vanilla-cache -o asset/source-cache
mlo_merger --generate-source-cache --exclude 'resourceName,[gabz]/shops'
mlo_merger --merge --vanilla-cache asset/vanilla-cache --source-cache asset/source-cache -o asset/merged
mlo_merger --deploy -i asset/merged --source-cache asset/source-cache -o asset/merged_mlo

mlo_merger --restore-source-cache -i asset/source --source-cache asset/source-cache
```

Source-cache selection accepts a resource name or input-relative resource path;
`-f` forces that selection, not its prerequisites. Replaced cached files are
retained under `_old/`. Files are cached at
`resources/{resourceName}/{stream-relative-path}`, without the leading
`stream/` or `streams/` component.

Source-cache generation, merge and the default pipeline accept `--exclude 'name,partial/path'`.
Each trimmed entry matches a case-insensitive substring of the resource path
relative to `-i`; resource names and group paths can both be used. Backslashes
are normalized to `/`; entries are literal text, not globs. Excluded resources
are not inventoried, fingerprinted, moved, or included in source conflict reports.
Exclusions apply only to the current execution and are not saved or inherited.
Omitting `--exclude` or passing `--exclude ''` means no exclusions. Pipeline and
merge pass the current exclusions to their source-cache prerequisite updates;
additional light edits use only resources recorded by that source cache. Legacy
`source_exclusions.json` files are ignored and removed after successful source-cache
updates. Previously cached excluded resources are removed
from the active JSON indexes and their cache directories are archived under
`_old/`, keeping those files out of merge and clone deployment. Files remaining
in source are untouched. Omitting exclusions does not automatically restore
archived files to source.

`--restore-source-cache` moves recorded active cache files back to their original
resource paths under `-i`, preserving modification times. Different existing
destination contents, missing cached files and symlink paths are rejected before
moving any files; identical destination files are retained and their cached
duplicates removed. The command leaves `_old/` history and unrecorded files
untouched and invalidates source-cache indexes after success. Run it while no
other cache or pipeline operation is active. A subsequent source-cache or pipeline
run will index the resources again and move vanilla-named files back into the cache.
It does not change merged or deployed output.

Merge writes native files under `ymap/` and `ybn/`, plus `_omit.txt` for replaced
source paths and `duplicates.json` for competing YMAP entity changes. It rebuilds
only changed or invalid groups. Failed merges leave the prior output intact.

Deploy consumes existing merge and source-cache data without regenerating them:

- Merged files: `stream/{extension}/merged/{filename}`.
- Remaining cached files: `stream/{extension}/clone/{resourceName}/{stream-relative-path}`.
- `files.txt`: original source-relative paths of active moved files and custom files replaced by light edits.

Deploy skips unchanged copies, repairs missing/changed outputs, and removes stale
managed files. Unrelated files are preserved; locally modified stale files are
rejected rather than deleted. Empty managed stream directories are pruned.
Deploy also creates `fxmanifest.lua` with the FiveM map declaration and
`/assetpacks` dependency when absent; existing manifests are never overwritten.

Run `mlo_merger --help` for options and per-command output defaults. To extract
a limited vanilla archive, use the explicit generator's `--gamebuild` stage
selector; numeric GTA build-ID mapping is not implemented. Convenience aliases
are in [.cargo/config.toml](.cargo/config.toml).

### Export And Remove LOD Lights

```bash
mlo_merger --to-json 'asset/source/[group]/resource_name/stream/nested/map_lodlights.ymap' --resource 'resource_name'
mlo_merger --merge --overwrite-dir asset/overwrite
```

`--to-json` exports a light-only view, not the complete YMAP. Use paths and
resource IDs from the R-tree `source` entries; active source-cache paths also
work. `--resource` also accepts a unique resource name without bracket groups;
ambiguous names require the full source-relative ID. JSON identity and output
paths always retain the full resource ID. Output defaults to
`asset/read_cache/ymap/<resource>/<stream-relative-path>.json`.
The sidecar caches input/output mtime, size, SHA-256 and light relationships;
unchanged exports skip native parsing and JSON writing. `-f` forces verification
and export. Do not run source-cache moves or restoration concurrently with exports.

The JSON joins child LOD arrays to parent distant arrays by their original index
within the same resource. Each logical light has explicit file references and
precomputed world-space OBB/AABB data. OBBs conservatively enclose the influence
volume; they are not exact rendered lighting. Unresolved relationships are
diagnosed and cannot be removed. No vanilla or other-resource parent fallback
is performed. The stream R-tree retains file bounds; light influence OBB/AABBs
are available separately in the on-demand light JSON.

The external editor writes
`asset/overwrite/<resource>/<stream-relative-path>.diff.json` with
`format: "mlo_lod_light_edits"`, `schemaVersion: 1`, the export's `resource`,
`path` and `files`, and `operations: [{"op":"remove","lightId":"..."}]`.
Copy IDs from editable exported lights; do not construct them from direction,
position or hash. Types are in [asset/gen_mlo_merger.ts](asset/gen_mlo_merger.ts).

Merge and the default pipeline accept `--overwrite-dir`. Edits change only the
selected resource's light contribution: another resource retaining the light
can keep it in the result. Linked arrays are compacted together; original source
and cached bytes are untouched. Changed dependencies require a fresh export and
diff. Unsupported edits, ambiguous relationships and custom non-light conflicts
fail before merged output publication. Removing a diff withdraws the edit on the
next merge/deploy. Applying native light edits currently requires RSC7 YMAPs;
PSO inputs can be inspected but are not edited.

Overwrite resolves resource and file identities exclusively from the active
source-cache inventory. Recorded custom YMAPs are read from their original
source paths; moved YMAPs are read from their recorded cached paths. Unrecorded
resources and files are not rediscovered, and diffs for inactive resources are
ignored. Rebuild source-cache to add or relocate inputs before applying edits.

Merge saves validated overwrite preparation in
`<source-cache>/overwrite_cache_info.json`. Unchanged mtime/size reuse stored
SHA-256 values; unchanged diff contents, source contents and resource membership
reuse prepared edits without parsing diffs or decoding related YMAPs again.
Timestamp-only changes refresh fingerprints without rebuilding. Diff additions,
removals, changed recorded inputs or cache resource/file membership invalidate preparation; `-f`
bypasses reuse. The cache is updated only after a successful merge, and a missing
or unreadable preparation cache is rebuilt. Diff enumeration and recorded-input
stat checks still run; overwrite does not scan source stream directories.

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
mlo_merger --find entity-guid 2443198849 --filter '*cs4_10_strm_0.ymap'
mlo_merger --find entity-position 3.5,4.2,0.0 --radius 1.0
mlo_merger --find ybn-position 1211.126,-507.5948,67.54723 --radius 1.0 --type box
mlo_merger --find file-from-position 1211.126,-507.5948,67.54723
mlo_merger --find occlude-position 3.5,4.2,0.0 --radius 1.0
```

Searches write XML to stdout without running the pipeline or modifying artifacts.
`entity-guid` searches by entity GUID. Position searches accept `X,Y,Z`; use
`--radius` for `entity-position`, `ybn-position` and `occlude-position`
(default: `1.0`). `file-from-position` finds files whose spatial bounds contain
the position and does not accept `--radius`.

`ybn-position` searches world-space collision shape centers, not any point
inside a shape. `--type` accepts `box`, `triangle`, `sphere`, `capsule` or
`cylinder`; omit it to search all supported shapes. Unsupported shapes are
skipped.

Use `-i` to select a merged directory (default: `asset/merged`). `--filter`
accepts a case-insensitive filename glob; quote wildcard patterns. `--all`
includes recorded vanilla and source inputs from before the merge. Duplicate
occurrences are retained.

Position searches include the 3D radius boundary. Negative coordinates are
supported; non-finite/out-of-range coordinates and negative or non-finite radii
are rejected. For entities, radius `0` selects the exact native-precision
position. `--all` results identify the input resource and path. A changed input
fingerprint causes an error requiring a fresh merge. This is a snapshot
comparison, not a computed semantic diff. YBN indices are input-local and may
change on rebuild.

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