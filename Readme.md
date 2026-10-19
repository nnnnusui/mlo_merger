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
Overlapping artifact directories are rejected. The source root
can contain them: for example, use `resources` as source while running the tool
from `resources/mlo_merged`.

Pipeline `-f` forces the derived caches, merge and deployment, but reuses an
existing raw vanilla archive. Pipeline-wide `--gamebuild` is not implemented; run
a single stage with its own operation below.

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
merge pass the current exclusions to their source-cache prerequisite updates. Legacy
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

Each YMAP is merged by reading every source's difference from the latest vanilla YMAP
from the source cache's `diff/{resource}/{stream-relative-path}.diff.json` and applying
them together to vanilla. Source-cache generation writes these documents and keeps them
while their source and vanilla files and the matching tolerances are unchanged; merge
fails when one is missing or stale, so rebuild the source cache first (`--merge` and the
pipeline do). The differences of several sources combine as follows:

- Array items are paired as the source cache's YMAP diffs pair them. Removed and
  modified items are named by their vanilla index, and an unpaired source item takes
  the place of an unpaired vanilla item between the same kept items (a replacement),
  or is added right after the vanilla item preceding it. Kept items stay in vanilla
  order, so a source's reordering of kept items is not reproduced.
- A removal wins over every other change of that vanilla item, including another
  source's replacement of it. Otherwise the first source to modify or replace an item
  wins, except occlude models, which take each field from the first source changing it
  and every source's triangle changes. Additions keep their place, in source order.
- `contentFlags` are OR-ed into vanilla's; the parent and IMAP link keep the first
  change. Added physics dictionaries and prop instances skip names already present;
  dictionary removals are not applied. Name, flags, extents, block and the distant LOD
  light counts keep their vanilla values.
- Entity `parentIndex` values are compared and merged as the entity they point at, read
  from the source resource's parent YMAP or else vanilla's, and written back as indices
  into the final entity arrays. Unmodified child YMAPs whose parent's entity order
  changed are rebuilt with relinked indices.

### Overwrite Source Differences

Merge and the default pipeline read overwrites from `--overwrite` (default:
`asset/overwrite`). `resources/{resourceName}.json` (`ResourceOverwrite`) lists,
per stream-relative path of that resource, the differences of its source-cache diff
document to leave out of the merge:

- In an object, `true` leaves out the whole field's difference; a nested selector
  only the parts it names.
- In an array, each selector leaves out the items whose written fields all match,
  such as `{"Removed": {"index": 0}}` or an entity's `guid`; its `changes` leaves out
  only the changes it names.

An overwrite applies only while the source and vanilla files match its `source` and
`vanilla` fingerprints (the diff document's `sha256` and `size`); otherwise it is
skipped with a warning. See [docs/sample/overwrite](docs/sample/overwrite/README.md).

`vanilla/latest/ymap/{fileName}.diff.json` (`VanillaOverwrite`) holds differences
written directly against a latest vanilla YMAP, in the shape of a diff document's
`diffs`. They are merged as one more source after every resource's (with their
resource overwrites applied), under the same merge policy, and also into vanilla
YMAPs no resource replaces. One applies only while the latest vanilla file matches
its `vanilla` fingerprint. Overwrites currently only apply to YMAPs.

### Deploy

Deploy consumes existing merge and source-cache data without regenerating them:

- Merged files: `stream/{extension}/merged/{filename}`.
- Remaining cached files: `stream/{extension}/clone/{resourceName}/{stream-relative-path}`.
- `files.txt`: original source-relative paths of active moved files.

Deploy skips unchanged copies, repairs missing/changed outputs, and removes stale
managed files. Unrelated files are preserved; locally modified stale files are
rejected rather than deleted. Empty managed stream directories are pruned.
Deploy also creates `fxmanifest.lua` with the FiveM map declaration and
`/assetpacks` dependency when absent; existing manifests are never overwritten.

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
ymap_box_occluder_rotation = 1
ymap_car_generator_position_decimals = 3
ymap_lod_light_position_decimals = 3
ymap_grass_position = 0.01
```

`ybn` controls inclusive per-axis polygon coordinates and radius matching across
all supported polygon types, not only boxes. `ymap` is the smallest change of a YMAP
float field that counts as a change: entity positions, rotations, scales and LOD
distances, car generator, time cycle modifier and distant LOD light values. Extents and
occlude model corners compare exactly. Occlude models use their XY extent
tolerance; box occluders use integer stored-coordinate units. YMAP comparisons
remain exclusive of the boundary. `ymap_box_occluder_rotation` ignores `iCosZ`/`iSinZ`
changes up to and including that many stored units (0 compares exactly), and
`ymap_car_generator_position_decimals` (0 to 6) sets the decimal places car generator
positions are rounded to when pairing generators; `ymap_lod_light_position_decimals` (0 to 6)
does the same for distant LOD light positions. `ymap_grass_position` is the per-axis
world-space distance, in metres, within which grass instances are treated as the same one.
These settings do not change search radii.

All other values must be positive; floating values must also be finite. Unknown keys
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

### Read Stream File Models

```bash
mlo_merger --read --resource brofx_mansion_06 apa_ch2_occl_05.ymap
mlo_merger --read --resource '[gabz]/cfx-gabz-247'
mlo_merger --read --vanilla apa_ch2_occl_05.ymap
mlo_merger --read --vanilla mpheist hei_ch1_11.ymap
mlo_merger --read apa_ch2_occl_05.ymap
```

`--read` writes each YMAP's model as minified JSON (`YmapReadDocument`), with
the path and fingerprint of the file it was read from and of the
`hash_names.json` that named its hashes. A resource's file is written to
`asset/read-cache/resources/{resourceName}/{stream-relative-path}.json` and a
vanilla file to `asset/read-cache/vanilla/{version}/ymap/{fileName}.json`.
A JSON still describing its file is not written again; `-f` writes it anyway.

- `--resource NAME [FILE]` reads one resource, by name or source-relative ID,
  from the existing source cache. Without a file it reads all of the resource's
  YMAPs. A file name matching several of the resource's files is an error;
  give its stream-relative path instead.
- `--vanilla [VERSION] [FILE]` reads a version of the vanilla archive, the latest
  without one. A version is its ID or the name after the number, such as
  `mpheist`. Without a file it reads all of the version's YMAPs.
- A file alone is read from every resource holding it and from the latest
  vanilla. If any resource holds several files of that name, nothing is written.

The source cache and vanilla archive default to `asset/source-cache` and
`asset/vanilla` (`--source-cache`, `--vanilla-dir`); `-o` changes the output root.
The command prints a `ReadResult` JSON line listing each input, its output and
whether it was written.

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
conflicts, diff-cache reports, source-cache YMAP diff documents
(`YmapComparisonDiff`), overwrite documents (`ResourceOverwrite`), `--read`
documents and results (`YmapReadDocument`, `ReadResult`), version listings and matching/blacklist configuration. It
does not include YBN difference payloads or JSON Schema documents. The release workflow regenerates it before packaging; it is not
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

- `--get-diff SOURCE` writes the source YMAP's differences from the latest vanilla
  cache in the source cache's diff form; `--get-diff SOURCE TARGET` only prints a mock
  invocation.
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