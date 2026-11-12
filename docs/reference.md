# Command Reference

Every operation, its files and its behavior. Run `mlo_merger --help` for the full option
list and per-command defaults; convenience aliases are in
[.cargo/config.toml](../.cargo/config.toml). For calling the executable from a FiveM
resource, see [usage.md](usage.md); for the design, [ARCHITECTURE.md](ARCHITECTURE.md).

## Pipeline

```bash
mlo_merger --pipeline
mlo_merger --pipeline -i resources -o merged_mlo --game-dir /mnt/gtav
```

The pipeline extracts vanilla only when its archive is missing, updates the vanilla and
source caches, merges, then deploys. Unchanged stages are reused. `--game-dir` defaults
to `/mnt/gtav` and `--magic` to `./magic.dat`; both are used only for missing vanilla
data (see [Vanilla extraction](development.md#vanilla-extraction)). It prints one line
of JSON (`PipelineReport`, see [usage.md](usage.md#pipeline-output)) and logs everything
else on stderr.

| Path | Default | Option |
|---|---|---|
| Source resources | `asset/source` | `-i` |
| Raw vanilla archive | `asset/vanilla` | `--vanilla` |
| Derived vanilla cache | `asset/vanilla-cache` | `--vanilla-cache` |
| Source cache | `asset/source-cache` | `--source-cache` |
| Merged files | `asset/merged` | `--merged` |
| Overwrites | `asset/overwrite` | `--overwrite` |
| Overwrite cache | `asset/overwrite-cache` | `--overwrite-cache` |
| Deployment | `asset/merged_mlo` | `-o` |

Overlapping artifact directories are rejected. The source root can contain them: for
example, use `resources` as source while running the tool from `resources/mlo_merged`.

Caches record paths relative to their stores, so moving `asset/`, or naming the same
directories by other paths, keeps every stage current. A source root holding none of the
source cache's resources is rejected, since it is not the tree the cache was built from.

`-f` forces the derived caches, merge and deployment, but reuses an existing raw vanilla
archive. `--gamebuild` selects the game build the vanilla cache, and so merge, targets
(see [Game Builds](#game-builds)).

## Individual Operations

```bash
mlo_merger --generate-vanilla -i /mnt/gtav -o asset/vanilla --magic magic.dat
mlo_merger --generate-vanilla-cache --vanilla asset/vanilla -o asset/vanilla-cache
mlo_merger --generate-source-cache -i asset/source --vanilla-cache asset/vanilla-cache -o asset/source-cache
mlo_merger --generate-source-cache --exclude 'resourceName,[maps]/shops'
mlo_merger --generate-overwrite-cache -i asset/source --overwrite asset/overwrite -o asset/overwrite-cache
mlo_merger --merge --vanilla-cache asset/vanilla-cache --source-cache asset/source-cache -o asset/merged
mlo_merger --deploy -i asset/merged --source-cache asset/source-cache -o asset/merged_mlo

mlo_merger --restore-source-cache -i asset/source --source-cache asset/source-cache
mlo_merger --restore-source-cache resourceName '[maps]/resource_b'
```

Each operation brings its prerequisites up to date first; `-f` applies only to the
operation itself.

## Source Cache

**Source-cache generation moves vanilla-named YMAP/YBN files and every other YMAP out of
resource stream directories.** They remain available in the source cache; other files stay
in the original resources. Resource names must be unique across groups. Files are cached at
`resources/{resourceName}/{stream-relative-path}`, without the leading `stream/` or
`streams/` component. A cached file a resource replaces with a new version is kept by its
content as `history/{resourceName}/{stream-relative-path}/{sha256}.{extension}`, with its diff
document then, if any, as `{sha256}.diff.json`, so overwrites written against it can be
followed to the new version (see [Overwrites](#overwrites)); nothing removes history.

A stream file's name identifies it across resources and vanilla: files of one name are one
stream file shipped several times. The exception is a name every resource ships for itself,
`_manifest.ymf`: it is no conflict, replaces no vanilla file and is no
[added stream file](#added-stream-files), so each resource's stays in its resource.

Source-cache selection accepts a resource name or input-relative resource path; `-f`
forces that selection, not its prerequisites.

### Exclusions

Source-cache generation, merge and `--pipeline` accept `--exclude 'name,partial/path'`.
Each trimmed entry matches a case-insensitive substring of the resource path relative to
`-i`; resource names and group paths can both be used. Backslashes are normalized to `/`;
entries are literal text, not globs.

- Excluded resources are not inventoried, fingerprinted, moved, or included in source
  conflict reports.
- Exclusions apply only to the current execution and are not saved. Omitting `--exclude`
  or passing `--exclude ''` means no exclusions. Pipeline and merge pass them to their
  source-cache updates.
- Previously cached excluded resources are removed from the active indexes and their
  cache directories are archived under `_old/`, keeping those files out of merge and
  clone deployment. Omitting exclusions later does not restore archived files to source.
- Legacy `source_exclusions.json` files are ignored and removed after successful
  source-cache updates.

### Restoring Resources

`--restore-source-cache` moves recorded active cache files back to their original
resource paths under `-i`, preserving modification times.

- Different existing destination contents, missing cached files and symlink paths are
  rejected before any file moves; identical destination files are kept and their cached
  duplicates removed.
- `_old/`, `history/` and unrecorded files are left untouched. Merged and deployed output is
  not changed.
- Run it while no other cache or pipeline operation is active. The next source-cache or
  pipeline run indexes the resources again and moves the same files back into the cache.
- Given resource names or input-relative resource paths, only those resources are
  restored. Every name is checked first; an unknown or ambiguous name is an error. The
  other resources stay cached, and `--read` still finds every file, but
  `stream_conflicts.json` and `vanilla_ymaps_to_read.json` are removed, so merges fail
  until the next source-cache or pipeline run.

## Merge

Merge writes native files under `ymap/` and `ybn/`, plus `_omit.txt` for replaced source
paths and `duplicates.json` for competing entity changes to vanilla YMAPs. It rebuilds only
changed or invalid groups. Failed merges leave the prior output intact.

Merge applies every source's differences from the latest vanilla file together to
vanilla. YMAP differences are read from the source cache's
`diff/{resource}/{stream-relative-path}.diff.json`, which source-cache generation keeps
while their source and vanilla files and the matching tolerances are unchanged; a missing
or stale document fails the merge instead of being recomputed, so rebuild the source cache
first (`--merge` and the pipeline do). Only YMAPs that still differ from vanilla after
merging are written.

### Added Stream Files

A stream file no vanilla file is named after, one a resource adds, is diffed and merged like
the others against an empty file of its format, so everything in the source is an addition.
YMAPs are the only format whose added files are merged so far; the others are deployed as
they are.

- Of resources adding a file of the same name, every version is merged, from the resource
  whose name sorts last (the one loaded last), so its version wins conflicts, and an addition
  an earlier version makes already is left out. The merged file is always written, so it
  replaces the others. The overwrite cache's `added_stream_files.json` (`AddedStreamFiles`)
  records each added file's sources in merge order and the additions left out, each with the
  item kept in its place.
- A resource overwrite with `ignore` leaves its stream file out of the merge.
- An added file only one resource ships is written only when the merge changes it, for
  example through an overwrite or a repaired parent reference; otherwise the resource's
  file is deployed as it is.

For an added YMAP:

- The empty baseline has no name, flags, extents or block, so the source's are kept.
- An addition an earlier version makes already is an entity with the same GUID or of the
  same type, archetype and LOD level at the same position (within the `ymap`
  [matching tolerance](#matching-configuration)), its children then pointing at the kept
  one, or another item equal within the matching tolerances; LOD lights and distant LOD
  lights are kept. The extents cover every version's.
- The source cache's `added_ymap_relationships.json` lists the added YMAPs, as
  `{resource}/{fileName}`, under the parent each declares, as the vanilla cache's
  `ymap_relationships.json` does; related YMAPs are merged together.

### Combining Sources

- A removal wins over every other change of that vanilla item, including another source's
  replacement of it, whatever the source order. An added entity equal to a removed vanilla
  entity is dropped too, so deleting wins over re-adding.
- Otherwise the first source to modify or replace an item wins; resource priority is not
  configurable. Occlude models are the exception: each field comes from the first source
  changing it, and every source's triangle changes are applied.
- Removed and modified items are named by their vanilla index. An unpaired source item
  takes the place of an unpaired vanilla item between the same kept items (a
  replacement), or is added right after the vanilla item preceding it; additions keep
  source order. Kept items stay in vanilla order, so a source's reordering of kept items
  is not reproduced.

### YMAP Fields

- `contentFlags` are OR-ed into vanilla's; the parent and IMAP link keep the first
  change. Resources' changes to name, flags, extents, block and the distant LOD light
  counts are left out, keeping vanilla's values.
- Added physics dictionaries and prop instances skip names already present; resources'
  dictionary removals are not applied, and their replacements are applied as additions.
- A [vanilla overwrite](#overwrites) is not limited this way: its changes to these
  fields apply. To keep a resource's value of one of them, write it into the vanilla
  overwrite.
- Entity extensions (light effects, doors, spawn point overrides and cloth bounds) are
  compared as a whole, ignoring number changes below the `ymap` tolerance; a change
  replaces all of the entity's extensions with the source's. A YMAP with another
  extension type fails to load rather than lose it.
- `[prefix_]lodlights_*.ymap` and `[prefix_]distlodlights_*.ymap` hold the `lodLights`
  and `distantLodLightsSOA` of the same lights, read by index, so a light removed from
  either is removed from both. The source cache adds the other YMAP's removals to each
  one's diff as `{"Removed": {"base_index": i, "implied": true}}`, and the document's
  `partner` records the files they were taken from. A resource shipping only one of the
  two has its removals merged into the other's vanilla YMAP as well. Pairs whose vanilla
  YMAPs differ in light count are left alone.

### Newer DLC Generations

DLCs ship vanilla YMAPs again under a prefix of their own, such as `lodlights_medium031`,
`hei_`, `apa_`, `vw_` and `m25_2_lodlights_medium031`, and the game loads every
generation. A change a resource makes to one generation's item is therefore carried over
to the same item of each newer generation, so a removed light or entity does not stay
visible through another copy.

- The vanilla cache groups YMAPs into generations by prefix and into families of the
  same name apart from the prefix and a trailing number, such as every
  `*distlodlights_small###`, since newer generations move items between those blocks.
  Generations are ordered by the vanilla stage first shipping one of their YMAPs.
- Items are the same when they are of one kind, within 0.01 of each other on every axis,
  and identified alike: entities by archetype, LOD lights by color and LOD light hash, car
  generators by model, box occluders by size and rotation, occlude models by flags,
  triangle count and maximum corner, time cycle modifiers by name. An item is paired only
  when exactly one item of the other generation fits, and no other item fits that one.
- Each pair of generations of a family is stored as a unit,
  `generation-units/{key}.json`, keyed by the contents of its YMAPs, and
  `generations.json` lists the units of the selected game build. Units are kept across
  rebuilds and game builds, so a rebuild pairs only the YMAPs that changed.
- Source-cache generation writes `propagated/{resourceName}/{fileName}.diff.json`
  (`YmapPropagatedDiff`) for each vanilla YMAP of a newer generation that the resource's
  changes reach and that it does not ship. Each carried removal or modification names
  the item it came from in `reason`, such as
  `"propagated from resourceName: vw_lodlights_medium031.ymap lod_lights[14]"`. A
  light removed from either YMAP of a LOD light pair is removed from both YMAPs of the
  newer pairs.
- Merge merges each document into its vanilla YMAP like one more resource diff, under
  the same policy; a stale document fails the merge.
- Only removals and modifications are carried. Additions and replacements, entity parent
  changes and modifications of nested arrays such as occlude model triangles are not,
  and neither are changes to items with no counterpart in a newer generation. When
  several changes reach one item, a removal wins and otherwise the first is kept.

### Entity Parents

- `parentIndex` is compared and merged as the entity it points at, read from the source
  resource's parent YMAP or else vanilla's, and written back as an index into the final
  entity arrays. Unmodified child YMAPs whose parent's entity order changed are rebuilt
  with relinked indices.
- An entity whose parent entity was removed, or whose parent lies in a YMAP the merged
  map's `parent` no longer names, is detached with a warning: `parentIndex` becomes `-1`,
  flag 8 (parent in the parent YMAP) is cleared, and `LODTYPES_DEPTH_HD` becomes
  `LODTYPES_DEPTH_ORPHANHD`; other LOD levels are kept.
- An index into a parent YMAP the merge cannot read is kept as it is.
- A parent without a unique, non-zero GUID is kept by index only while the array it
  indexes is unchanged; otherwise that merge group fails rather than guess.

### YBN

- Shapes matching vanilla that any source lacks are removed, and new shapes from every
  source are combined. Removal wins over material or triangle-corner flag changes of the
  same geometry, whatever the source order.
- Shapes match by geometry within the coordinate tolerance, not by polygon or vertex
  index; boxes match by their eight corners, so a different reference vertex does not
  bring back a removed box.
- Files without a vanilla baseline are skipped, or rejected when sources conflict.

## Overwrites

Overwrites are the only inputs written from outside the tool besides the sources. They are
read from `--overwrite` (default: `asset/overwrite`), and what they call for is derived
into the overwrite cache (`--overwrite-cache`, default `asset/overwrite-cache`) by
`--generate-overwrite-cache`, which `--merge` and `--pipeline` run first. The merge always
reads it, even with no overwrites, since it also records which resources each added stream file is
merged from; a missing one, or one older than the source and vanilla caches or the
overwrites, fails the merge. Overwrites currently only apply to
YMAPs; one of another format is reported as `unsupported_format` and not applied.

- `resources/{resourceName}/{stream_path}.diff.json` (`StreamFileOverwrite`), one per
  stream file, lists the differences of that file's diff document to leave out of the
  merge, or sets `ignore` to leave the file out whole. The diff document is the source
  cache's, against vanilla or, for an [added stream file](#added-stream-files), an empty file of
  its format.
- `vanilla/{version}/ymap/{fileName}.diff.json` (`VanillaOverwrite`) holds differences
  written directly against the latest vanilla YMAP at `{version}/ymap/{fileName}` in the
  vanilla archive, and applies only while that version is the latest. They are merged as one more source
  after every resource's, under the same merge policy, and also into vanilla YMAPs no
  resource replaces. Unlike resources' changes, their name, flags, extents, block,
  distant LOD light count and physics dictionary changes apply (see
  [YMAP Fields](#ymap-fields)).

An overwrite leaving out differences applies only while the files it was written against are current: its
`target` and `base` fingerprints (`sha256` and `size`) must match. Otherwise it is
left out whole with a warning, and the pipeline lists it in `ignored_overwrites` on every
run until it is rewritten. A resource overwrite whose source alone changed is first
followed to the new version: the overwrite cache compares the diff document of the version it
was written against, from the source cache's history, with the current one, and rewrites the
overwrite in place when every difference it left out is still left out by the same selector
or no longer made, dropping the selectors that match nothing; otherwise it is left as it is.
Either way `{overwrite cache}/source_updates/` records it (see
[usage.md](usage.md#when-source-or-vanilla-files-change)). The overwrite cache's
`warnings.json` (`OverwriteWarnings`)
lists these; see
[usage.md](usage.md#overwrite-warnings). The document formats, selectors and what happens when files
change are described in [usage.md](usage.md#overwrite-documents); complete examples are in
[docs/sample/overwrite](sample/overwrite/README.md).

## Deploy

Deploy consumes existing merge and source-cache data without regenerating them:

- Merged files: `stream/{extension}/merged/{filename}`.
- Remaining cached files: `stream/{extension}/clone/{resourceName}/{stream-relative-path}`.
- `files.txt`: original source-relative paths of active moved files.

Deploy skips unchanged copies, repairs missing or changed outputs, and removes stale
managed files. Unrelated files are preserved; locally modified stale files are rejected
rather than deleted. Empty managed stream directories are pruned. Deploy also creates
`fxmanifest.lua` with the FiveM map declaration and `/assetpacks` dependency when
absent; existing manifests are never overwritten.

## Game Builds

`--gamebuild` takes a FiveM game build number (`sv_enforceGameBuild`) or a DLC name listed
for one under `[gamebuild]` in [asset/config.toml](../asset/config.toml), such as `3095` or
`mp2023_02`. Names are spelled as
`sv_enforceGameBuild` accepts them: `xm18`, `h4` and `sum` mean `mpchristmas2018`,
`mpheist4` and `mpsum`. Stage IDs and positions are not accepted. The table follows FiveM's
sources (the builds in `code/client/launcher/GameCache.cpp` and the names in
`code/components/citizen-server-impl/include/GameBuilds.h`); add a build there when FiveM
supports a new one.

- A build number missing from the table uses the closest listed build below it, with a
  warning, so a build newer than the table targets the newest one listed.
- A build that adds no DLC pack is given by its number: FiveM's name for it, such as
  `mp2026_patch_01` for 3788, is not a stage and is not accepted.
- A build ends at the last stage before the next build's first listed DLC in
  `dlclist.xml` order, so it includes its own patch and `_g9ec` stages; a name listed
  under several builds selects the latest. A build without DLC names (such as 3323)
  includes what the build before it does, and build 1 stops at the title update.
- The build applies only to the vanilla-cache step: `--generate-vanilla` always extracts
  every stage, and the vanilla cache selects the latest files from the stages up to the build's
  last one. The base and title-update files come from the installed game, not from the
  target build.
- The pipeline, `--generate-source-cache` and `--merge` pass their `--gamebuild` to the
  vanilla-cache step they run first, so running one without it returns the cache to the
  last stage.

## Matching Configuration

Merge and diff-cache operations read [asset/config.toml](../asset/config.toml) once per
operation.

The tolerances keep rounding from passing for a change. Tools round values differently when
they save a file: CodeWalker and OpenIV, for example, write the same position with slightly
different digits. A value a resource never touched can therefore differ from vanilla by that
rounding alone, and would otherwise be detected, and merged, as a change. Differences within
the tolerances are treated as no change.

Missing files or omitted keys use these defaults:

```toml
[tolerance]
ybn_position = 0.05
ymap = 0.001
ymap_occlude_model_position = 0.01
ymap_box_occluder_position = 1
ymap_box_occluder_rotation = 1
ymap_car_generator_position_decimals = 3
ymap_lod_light_position_decimals = 3
ymap_grass_position = 0.01
```

| Key | Meaning |
|---|---|
| `ybn_position` | Inclusive per-axis polygon coordinate and radius matching, for every supported polygon type |
| `ymap` | Smallest change of a YMAP float field that counts: entity positions, rotations, scales and LOD distances, car generator, time cycle modifier and distant LOD light values |
| `ymap_occlude_model_position` | Occlude model XY extent tolerance; corners compare exactly |
| `ymap_box_occluder_position` | Box occluder center tolerance, in integer stored-coordinate units (a quarter metre) |
| `ymap_box_occluder_rotation` | `iCosZ`/`iSinZ` changes up to and including this many stored units are ignored (0 compares exactly) |
| `ymap_car_generator_position_decimals` | Decimal places (0 to 6) car generator positions are rounded to when pairing |
| `ymap_lod_light_position_decimals` | The same for distant LOD light positions |
| `ymap_grass_position` | Per-axis world-space distance, in metres, within which grass instances are the same one |

- Position tolerances end in `_position`. The former names `ybn`, `ymap_occlude_model` and
  `ymap_box_occluder` are still read.
- YMAP comparisons are exclusive of the boundary; extents compare exactly. These settings
  do not change search radii.
- All other values must be positive; floating values must also be finite. Unknown keys and
  invalid TOML are errors.
- Effective settings are part of merge freshness, so changing them regenerates on the
  next merge without `-f`. Larger tolerances can treat intentionally different nearby
  shapes as identical.

## Inspect Existing Data

```bash
mlo_merger --find entity-guid 2443198849 --filter '*cs4_10_strm_0.ymap'
mlo_merger --find entity-position 3.5,4.2,0.0 --radius 1.0
mlo_merger --find ybn-position 1211.126,-507.5948,67.54723 --radius 1.0 --type box
mlo_merger --find file-from-position 1211.126,-507.5948,67.54723
mlo_merger --find occlude-position 3.5,4.2,0.0 --radius 1.0
```

Searches write XML to stdout without running the pipeline or modifying artifacts.

- `entity-guid` searches by entity GUID. Position searches accept `X,Y,Z`; use `--radius`
  for `entity-position`, `ybn-position` and `occlude-position` (default: `1.0`).
  `file-from-position` finds files whose spatial bounds contain the position and does not
  accept `--radius`.
- `ybn-position` searches world-space collision shape centers, not any point inside a
  shape. `--type` accepts `box`, `triangle`, `sphere`, `capsule` or `cylinder`; omit it
  to search all supported shapes. Unsupported shapes are skipped.
- `occlude-position` measures box occluders from their center, converted from stored
  quarter-metre units, and occlude models from the nearest point of their bounds.
- `-i` selects a merged directory (default: `asset/merged`). `--filter` accepts a
  case-insensitive filename glob; quote wildcard patterns. Duplicate occurrences are kept.
- With `--all`, the inputs the merge recorded are read from `--source`, `--source-cache` and
  `--vanilla-cache` (defaults under `asset/`), which the merge records names relative to.
- `--all` includes recorded vanilla and source inputs from before the merge, identified
  by resource and path. A changed input fingerprint is an error requiring a fresh merge.
  This is a snapshot comparison, not a computed semantic diff.
- Position searches include the 3D radius boundary. Negative coordinates are supported;
  non-finite or out-of-range coordinates and negative or non-finite radii are rejected.
  For entities, radius `0` selects the exact native-precision position. YBN indices are
  input-local and may change on rebuild.

## Read Stream File Models

```bash
mlo_merger --read --resource resourceName apa_ch2_occl_05.ymap
mlo_merger --read --resource '[maps]/resource_b'
mlo_merger --read --vanilla apa_ch2_occl_05.ymap
mlo_merger --read --vanilla --gamebuild 2802 hei_ch1_11.ymap
mlo_merger --read apa_ch2_occl_05.ymap
mlo_merger --read apa_ch2_occl_05.ymap hei_ch1_11.ymap
mlo_merger --read --all
```

`--read` writes each YMAP's model as minified JSON (`YmapReadDocument`), with the path
and fingerprint of the file it was read from and of the `hash_names.json` that named its
hashes. A resource's file is written to
`asset/read-cache/resources/{resourceName}/{stream-relative-path}.json` and a vanilla
file to `asset/read-cache/vanilla/{version}/ymap/{fileName}.json`, where `{version}` is the
version folder holding the file in the vanilla archive: the last version, up to the stage
read, that changed it. Versions holding the same file share its document. A JSON still
describing its file is not written again; `-f` writes it anyway.

Vanilla files are read from the stage the vanilla cache was built for (`latest_version` in
`asset/vanilla-cache/cache_info.json`, `--vanilla-cache` to change the cache), so they match
the merge and the cache's `stream_file_rtree.json`; without a vanilla cache, or one built
for a stage the archive lacks, from the last stage. `--gamebuild BUILD` reads them as of a
game build instead, resolved as for the other commands (see [Game Builds](#game-builds)).
`--gamebuild` changes only the vanilla files.

- `--resource NAME [FILE...]` reads one resource, by name or source-relative ID, from the
  existing source cache. Without a file it reads all of the resource's YMAPs. A file name
  matching several of the resource's files is an error; give its stream-relative path
  instead.
- `--vanilla [FILE...]` reads vanilla files only. Without a file it reads all of the
  stage's YMAPs.
- Files alone are read from every resource holding them and from vanilla. If any resource
  holds several files of one name, nothing is written.
- `--all` reads every YMAP of every resource and of vanilla; it takes no file names.
- Several files are read together in the order given; a file named twice is read once.
  If any name matches no file, nothing is written.
- Files are read and written in parallel; the result keeps the order above.

The source cache and vanilla archive default to `asset/source-cache` and `asset/vanilla`
(`--source-cache`, `--vanilla-dir`), and resource files not moved into the source cache are
read from `--source` (default: `asset/source`); `-o` changes the output root. Each document
records the file it was read from relative to its store. The command prints a
`ReadResult` JSON line listing each input, its output and whether it was written.

## Check The Stores

```bash
mlo_merger --check
mlo_merger --check --deep --merged asset/merged -o asset/merged_mlo
```

`--check` compares every store with its own metadata without changing any file, prints one
line of JSON (`CheckReport`) and fails when it finds an inconsistency. Each issue names its
`store`, its `kind` and the `path` relative to the store; stores whose directory does not
exist are skipped. Store paths take the same options and defaults as the other commands.

| `kind` | Meaning |
|---|---|
| `missing` | The metadata records the file, but it does not exist |
| `unrecorded` | The file exists, but the metadata does not record it, such as a stray file in `source-cache/resources/` or a leftover staging directory |
| `changed` | The file's size, or with `--deep` its SHA-256, differs from the record |
| `unreadable` | The store's metadata cannot be read, so the store was not checked further |
| `stale` | The overwrite cache was built from other source or vanilla caches or overwrites than the current ones |

- The vanilla archive, vanilla cache, source cache (including diff documents) and merge
  output are checked whole. In the deployment, only the managed `stream/{extension}/merged/`
  and `stream/{extension}/clone/` directories are checked for unrecorded files, since other
  files are left to the user.
- The overwrite cache is checked against what it was built from (`--overwrite`, default
  `asset/overwrite`), and for the files it names: the sources of `added_stream_files.json`
  in the source cache, and the earlier versions `source_updates/` records in the source
  cache's history; one that is missing is reported as `missing` on the record naming it.
- Without `--deep`, contents are compared by size; the vanilla archive and cache record no
  sizes, so only `--deep` compares their contents. `--deep` reads every file, a few seconds
  for a full vanilla archive.
- The vanilla cache holds no copies of the latest files: each is checked in the vanilla
  archive its metadata names, and reported by its path from the cache directory, such as
  `../vanilla/0000-base/ymap/{file name}`.
- Nothing is repaired. The next merge and deployment rebuild their missing or changed
  files. A damaged vanilla archive is reused as it is, so extract it again with
  `--generate-vanilla`; a source-cache file that is missing
  is restored from its resource when the resource still has it, and a stale overwrite cache
  is rebuilt by `--generate-overwrite-cache` or a command running it. Unrecorded files are left
  for you to remove.

## Convert Files

```bash
mlo_merger --to-xml collision.ybn -o exported
mlo_merger --from-xml exported/collision.ybn.xml -o rebuilt
mlo_merger --from-xml map.ymap.xml -o rebuilt --vanilla asset/vanilla
```

Inputs can be files or directories. Conversion output defaults to the current directory.
META resources need compatible schemas; `--vanilla` supplies schema or template inputs.
YBN conversion does not require an external schema directory.

## TypeScript Types

The release archives contain `asset/gen_mlo_merger.ts`, generated from the Rust Serde
models: merge diagnostics, vanilla/source/merge/deploy caches, stream conflicts,
diff-cache reports, source-cache YMAP diff documents (`YmapComparisonDiff`), overwrite
documents (`StreamFileOverwrite`, `VanillaOverwrite`), the overwrite cache's added stream files
and warnings (`AddedStreamFiles`, `OverwriteWarnings`), `--read` documents and results (`YmapReadDocument`,
`ReadResult`), pipeline output (`PipelineReport`), version listings and
matching configuration. It does not include YBN difference payloads or JSON
Schema documents, and it is not checked into the repository; see
[development.md](development.md#typescript-types) to generate it locally.

```typescript
import type { DuplicateReport, MatchingConfig } from "./asset/gen_mlo_merger";

const config: MatchingConfig = { tolerance: { ybn_position: 0.05 } };
```

Types preserve JSON field names, nullability and optional inputs. Paths are strings; sets
are arrays. File sizes and timestamps are JSON numbers, represented as TypeScript
`number`, not `bigint`; values above `Number.MAX_SAFE_INTEGER` need a precision-aware
JSON reader. The definitions do not validate data at runtime.

## Limitations

- Merge uses the latest selected vanilla, without inferring which vanilla version a
  source was made from. That inference is deferred: for YMAPs it matters only for MLOs
  made from a vanilla version older than mp2025_02 (apart from some LOD light YMAPs,
  versions up to patchday27ng, before GameBuild 2699), and it needs a way to track
  differences exactly. The code writing vanilla YMAP version-to-version diffs is kept but
  not called; see [vanillaDiff.md](vanillaDiff.md).
- Changes carried over to [newer DLC generations](#newer-dlc-generations) are taken from
  the source cache's diff documents, so a resource overwrite leaving a change out of a
  resource's diff does not leave it out of the newer generations. Changes are carried
  only to newer generations, not to older ones.
- `--get-diff SOURCE` writes the source YMAP's differences from the latest vanilla cache
  in the source cache's diff form; `--get-diff SOURCE TARGET` only prints a mock
  invocation.
- Cache stages do not reproduce the complete game-engine mount rules or historical
  installations.
- MLO semantic differences are reports, not lossless history patches.
- Conversion support depends on the resource family and available schemas. PSO
  rebuilding cannot grow arrays or allocated strings; RBF YMT is unsupported.
- Matching XML or stable rebuilds does not establish in-game load safety. Validate
  generated resources in the target game environment.
