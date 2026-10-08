# MLO Merger

[![SUSHI-WARE LICENSE](https://img.shields.io/badge/license-SUSHI--WARE%F0%9F%8D%A3-blue.svg)](https://github.com/MakeNowJust/sushi-ware)

Merges FiveM YMAP resources and conflicting YBN collision files against vanilla data.
See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the processing flow and
[docs/TODO.md](docs/TODO.md) for remaining work.

## Requirements

- Rust stable.

```bash
cargo build
```

## CLI

Without an operation flag, `cargo run` executes the complete pipeline:
generate vanilla only when its archive is missing, update vanilla-cache and
source-cache, merge, then deploy. Existing stages use their normal freshness
checks. `-i` selects source resources and `-o` selects the final deployment
directory, defaulting to `asset/source` and `asset/merged_mlo` respectively.
The source-cache stage moves vanilla-named resource files into its cache.

```bash
cargo run
cargo run -- -i asset/source -o merged_mlo
cargo run -- -i resources -o merged_mlo --game-dir /mnt/gtav
```

`--game-dir` is used only when vanilla extraction is needed (default `/mnt/gtav`).
Intermediate paths can be changed with `--vanilla`, `--vanilla-cache`,
`--source-cache`, and `--merged` (default `asset/merged`). Overlapping input and
artifact directories are rejected before processing. Pipeline `-f` forces the
derived caches, merge and deploy, but reuses an existing vanilla archive.
Pipeline `--step-name` selection and pipeline-wide `--gamebuild` ceilings are
not implemented and are rejected explicitly; use individual generator commands
to create a limited vanilla archive.

The command interface is being reorganized. `--generate-vanilla`,
`--generate-vanilla-cache`, `--generate-source-cache`, `--merge`, `--deploy`,
`--to-xml`, and `--from-xml` call core implementations. The diff command
currently only prints a mock invocation.

```bash
cargo run -- --generate-vanilla -i /mnt/gtav -o asset/vanilla
cargo run -- --generate-vanilla-cache --vanilla asset/vanilla -o asset/vanilla-cache
cargo run -- --generate-source-cache -i asset/source --vanilla-cache asset/vanilla-cache -o asset/source-cache
cargo run -- --generate-source-cache resourceName -i asset/source -f
cargo run -- --merge --vanilla-cache asset/vanilla-cache --source-cache asset/source-cache -o asset/merged
cargo run -- --deploy -i asset/merged --source-cache asset/source-cache -o merged_mlo
cargo run -- --to-xml collision.ybn -o exported
cargo run -- --from-xml exported/collision.ybn.xml -o rebuilt --vanilla asset/vanilla
cargo run -- --get-diff vanilla.ybn mod.ybn -o diff
cargo run -- --find entity-guid 2443198849 --filter lr_cs4_10_strm_0.ymap
cargo run -- --find entity-guid 2443198849 --filter lr_cs4_10_strm_0.ymap --diff-all
```

For generation, merge and deploy commands, `-o` defaults to the artifact directory shown
in each command's help. Conversion and diff commands default to the current
directory. Common options include `--vanilla`, `--vanilla-cache`,
`--source-cache`, `--gamebuild`, `-f` to force the selected operation, `-y` to
skip confirmation, and repeatable `--step-name <name>` for reserved stage
selection (not implemented yet). `--gamebuild` currently selects an installed version or
DLC stage name; mapping numeric game build IDs is not implemented yet.
`--vanilla` supplies source schema/template files to `--from-xml`. Run
`cargo run -- --help` for the full interface. Convenience aliases are defined in
[.cargo/config.toml](.cargo/config.toml).

`--generate-source-cache` discovers manifest-bearing resources under `-i` and
moves vanilla-named YMAP/YBN files from `stream` or `streams` into
`asset/source-cache/resources/{resourceName}/{stream-relative-path}` (or the selected
`-o` directory). Unmatched files stay in the original resource. Resource names
must be unique across groups. Replaced cached files and their previous
fingerprint/mtime records are moved to `_old/{UTC timestamp}/` inside the cache.
The `stream/` or `streams/` prefix is omitted from cached and archived paths;
the original paths remain in metadata for merge/omit output. Previously cached
files are migrated to this layout when their resource is next checked.
Legacy resource folders directly under the cache are moved under `resources/`
without changing file modification times. The `_old` layout remains unchanged.
Colliding relative paths from `stream` and `streams` are rejected without overwriting files.
Moving a resource between input groups without changing its name preserves
its cache; only the recorded source paths and resource keys are updated.

Each run checks resource directories, manifests, stream presence, and file
fingerprints, updating only affected resources. Resources without streams are
also recorded. An optional `resourceName` (or input-relative resource path)
restricts the check; `-f` forces that selection, not its vanilla prerequisites.
Cached files remain available to merge after extraction. The cache records
latest-vanilla matches, cross-resource conflicts and YMAP parent/child closure;
it does not generate diffs.

In the stream-conflict report, `conflicts` lists files currently in the active
source-cache resource directories using cache-relative paths; `_old` is excluded.
`source_conflicts` retains the original resource-relative conflict paths from
the source inventory, including moved files. The report's input directory and
scan/conflict counts refer to the source cache.

`--generate-vanilla` writes `rpf_names.json` and `hash_names.json` alongside the
raw archive. The single `names` map contains embedded YMAP names and entity
archetypes as `hash: text` entries; entity GUIDs are not included. Regenerate an older raw
cache once to populate the RPF-name candidates used to resolve prop names.

## Entity Search

`--find entity-guid <GUID>` searches existing merged native YMAPs and writes a
single XML document to stdout without running the pipeline or creating XML files.
Use `-i` for a custom merged directory and `--filter <glob>` for a
case-insensitive filename match. Literal filenames still match exactly. Glob
patterns support `*`, `?`, character classes and alternatives; quote them so the
shell does not expand the pattern. Omit the filter to search all merged YMAPs.
Duplicate GUID occurrences are retained.

```bash
cargo run -- --find entity-guid 2443198849 --filter '*cs4_10_strm_0.ymap' --diff-all
cargo run -- --find entity-position 3.5,4.2,0.0 --round 1.0
```

`--find entity-position X,Y,Z --round R` selects every entity whose 3D Euclidean
distance from the center is at most `R`, including the boundary. The default
radius is `1.0`; `0` selects the exact position at YMAP coordinate precision.
Coordinates use native `f32` precision and distances are calculated in `f64`.
Negative coordinates are supported. Non-finite/out-of-range coordinates and
negative/non-finite radii are rejected. Position searches also accept glob filters
and `--diff-all`, and XML identifies the requested center and radius.

`--diff-all` also reads vanilla and each source input recorded in
`merge_cache_info.json`; XML entries identify their stage, path and resource.
`found="false"` marks a stage without matching entities, including removed entities or
maps with no generated output. Recorded input fingerprints must still match;
otherwise the command fails and requests a fresh merge instead of presenting
changed inputs as historical data. This option outputs entity snapshots for
comparison, not a computed semantic diff.

## Incremental Merge

`--merge` stores source/vanilla mtime, size, SHA-256, dependency information and
output/no-output results in `merge_cache_info.json` under merged output. Only
changed or invalid groups are merged again: YBNs by basename, YMAPs by connected
parent/child relationships. Timestamp-only changes do not require remerging;
missing or changed generated files are rebuilt. Unchanged outputs retain mtime,
and removed inputs or newly unchanged results remove obsolete output and omit
entries. `-f` forces this merge stage without forcing its prerequisites.
Merge also writes `duplicates.json` for YMAP entities receiving multiple source
diffs. Entries are grouped by filename and include the original GUID, vanilla
snapshot, applied source and ignored sources with resource names, paths and
source entity snapshots. Reasons are `identical_duplicate`, `deletion_wins`,
or `first_change_wins`. This is an entity-diff report, separate from filename
conflicts. Cached groups retain their report entries, and missing reports are
restored from merge metadata without remerging unchanged data.

## Deployment

Merge writes native files under `asset/merged/ymap/` and `asset/merged/ybn/`,
with YMAP models written directly to RSC7 using embedded META schemas, without
intermediate XML.
`_omit.txt` stays at the merged root. `--deploy` copies existing outputs without
running merge or cache generation. Its default input is `asset/merged` and its
default output is `asset/merged_mlo`.

Deployment places merged files at `stream/{extension}/merged/{filename}` and
remaining source-cache files at
`stream/{extension}/clone/{resourceName}/{stream-relative-path}`. Cached files
with a case-insensitive basename already present in merged output are excluded
from clone; `_old` and cache reports are not deployed. Resource paths are retained
under clone so copies from separate resources do not overwrite each other.

The output's `deploy_cache_info.json` records source/destination mtime, size and
SHA-256. Unchanged copies are skipped, missing or modified copies are repaired,
and stale managed copies are removed; unrelated output files are preserved.
Modified stale copies are rejected instead of deleted. `-f` forces all copies.
After updating files, deployment removes empty directories under `stream/`
from the leaves upward, including old empty folders. Nonempty folders, symlinks,
the deployment root and its metadata are preserved.
Deployment also writes `files.txt`, a sorted, deduplicated list of original
source-relative paths for moved files still present in source-cache. This includes
both merged inputs and clone files, but excludes unmoved and archived files.
An unchanged list is not rewritten; without source-cache inventory it is empty.

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