# Architecture

## Overview

MLO Merger prepares vanilla data, merges supported FiveM map and collision
changes, and deploys the resulting stream files. Inspection and conversion are
separate operations.

Native Rust handles game-file conversion, merging and RPF reading for vanilla
generation. CodeWalker is used only by round-trip tests that compare conversion
with it, behind the `codewalker` feature.

## Code Format and Module Layout

`rustfmt.toml` is the source of truth for Rust formatting; verify changes with
`cargo fmt --check`.

Current module layout:

```text
src/
├── app/
│   ├── vanilla.rs       shared vanilla archive generation
│   ├── map/             map/MLO workflows and composition
│   └── clothing/        clothing workflows, including future composition
├── core/
│   ├── game_archive/    shared RPF/DLC extraction and versioned archive engine
│   ├── format/          game-file formats and codecs
│   └── ...              other reusable mechanisms
├── cli/                 argument parsing and dispatch to app
├── common/              cross-cutting helpers
└── platform/            OS and process abstractions
```

Shared archive extraction, manifest handling, validation and publication live
in `core/game_archive`. Map generation, caches, overwrite handling, merge,
deployment, inspection and store checks live in `app/map`; clothing collection
and its counts cache live in `app/clothing`; generation of the shared vanilla archive
lives in `app/vanilla`.

Command defaults place the shared vanilla archive, config and log directly under the
asset directory, and each workflow's stores under its own directory (`map/`, `clothing/`).

Production dependencies flow from `cli` to `app` to `core`; core runtime modules
do not depend on application features.

## Pipeline

`--pipeline` runs vanilla extraction when no raw archive exists, then
vanilla-cache, source-cache, overwrite-cache, merge and deploy in order. Existing raw vanilla is
validated and reused; missing vanilla requires an installed game path. Nonempty
directories without an archive manifest are not automatically replaced. Source
input is controlled by `-i`; final deployment output by `-o`. Intermediate caches
and merged output have independent paths. All artifact paths are checked for
overlap before any stage mutates data, and stages stop on the first error.

The pipeline sequences stages without inspecting their storage formats.
GenerateVanilla owns raw-archive validation and the generate-or-reuse decision;
explicit vanilla generation can still rebuild an existing archive. Overwrite
and artifact paths share one overlap check. Source nesting is intentionally
allowed for execution and artifacts under `resources/mlo_merged` with `resources`
as the source root. Source discovery skips deployments, identified by
`deploy_cache_info.json`.

Each stage retains its own freshness and prerequisite checks and writes the JSON
records the next stage needs. Generated JSON is compact and single-line; use `jq .`
to format it for inspection. A single stage runs through its own operation
(`--generate-vanilla`, `--generate-vanilla-cache`, `--generate-source-cache`,
`--generate-overwrite-cache`, `--merge`, `--deploy`), which brings its prerequisites up to date first.
Pipeline force applies to derived stages, not existing raw vanilla. The game build
applies only to the vanilla-cache stage; raw vanilla always holds every stage.

## Module Responsibilities

Each artifact has one owning module (`vanilla`, `vanilla_cache`, `source_cache`,
`overwrite`, `overwrite_cache`, merge and deploy). The owner reads its own files, checks their
freshness and exposes the data other stages need; consumers do not walk another
stage's directories or parse its records. Overwrite application is likewise
exposed by the overwrite module, and merge reads overwrite data only through it.
The vanilla archive, vanilla cache, source cache and merge output each record their
metadata in `cache_info.json`, which changes with every update of the store: its owner
exposes its path, and consumers fingerprint it to tell when what they derived is stale. Recorded paths are relative to the store holding each file
(resources and their files to the source root), and freshness compares contents, with
modification times only as a hint to skip hashing. Where the stores are, and how a command
names them, does not affect any stage. The source cache identifies its source tree by its
resources rather than its path: a source root holding none of them is rejected.
Format codecs (`format/*`) stay independent of artifact layout and batch processing.
The GUI, opened when the executable starts without arguments, is a front end for
vanilla generation only: it collects the game directory and `magic.dat` and runs the
same generation as `--generate-vanilla --content map`.

## Source Cache

Source-cache generation moves vanilla-named inputs and every YMAP from resources into the
cache, retaining original paths. Other files stay in the resource, as do the
directories the moved files leave empty; replacements archive prior cached files. Resource names must be unique, and colliding stream
paths are rejected. Scoped updates retain other inventories for global conflict
and relationship analysis. Missing cached inputs are errors.

For each cached YMAP that overrides a vanilla YMAP, the source cache writes a diff
document against the latest vanilla file. A YMAP that resources add, named after no vanilla
YMAP, is compared with an empty YMAP, so that its items are additions that overwrites can
leave out; the baseline depends on no source, so overwrites stay current while their own
source does. `added_ymap_relationships.json` records the added YMAPs' parents per resource.
`hash_names.json` names the hashes the resources' file names spell, with the LOD names
derived from their `.ydr` and `.ydd` files, so that `--read` names a resource's own YMAPs
and archetypes; it is built from the inventory alone, as YMAP and YTYP META strings add no
name the file names miss.
The document records both files'
fingerprints and the matching tolerances; it is reused while they are unchanged
and regenerated otherwise. Checking reads every document, so `diff/inputs.json`
records the diff layout, tolerances and vanilla cache revision the diffs were taken
under, with a digest of each resource's inventoried YMAPs. While those match, a
resource's existing diffs are kept unread; changed resources are checked file by file,
and every diff is when the rest changed. A diff of one YMAP of a LOD light pair also
carries the other's removals, marked implied, so its document records the other YMAP
(the resource's own, or vanilla's) and its baseline as `partner`, checked the same way.
The source cache also records which vanilla YMAPs a
merge must read: the parents and children related to changed files.

After taking the diffs, the source cache writes to `implied/` what keeps each resource's
LOD light pairs in step: the lights a YMAP adds that the other YMAP of its pair does not, to
leave out, and, for a resource shipping only one YMAP of a pair, the removals of the vanilla
other one. They are derived from the diff record and the vanilla cache revision, and are
kept while both are unchanged. The source cache's diffs and `implied/` documents describe
each source on its own; DLC generations are resolved by the merge (see
[Generation Resolution](#generation-resolution)).

## Generation Resolution

DLCs ship vanilla stream files again under prefixes of their own (`hei_`, `apa_`, `vw_`,
`m25_2_`), and the game loads one generation of each; a streamed file of another loads beside
it, placing its items or collision twice. Resolution is split by what each stage owns:

- The raw vanilla archive records what each DLC pack declares: its data files, the change
  sets the game executes, and the islands its island hopper files define.
- The vanilla cache applies those in stage order through the selected game build to find
  the archives the game loads (`dlc_activity.json`, `app::map::vanilla_cache::activity`).
  It groups YBNs into families without pairing anything, as their polygons match by geometry,
  and pairs the items of each YMAP family's generations, over the whole archive so that YMAPs
  first shipped after the build are paired too, one unit per older and newer generation,
  stored by a digest of its YMAPs so that rebuilds and game build switches reuse them. It
  names the loaded successor of each YMAP the game does not load in `ymap_generations.json`,
  the loaded YBNs overlapping each YBN it does not load in `ybn_generations.json`
  (`app::map::vanilla_cache::ybn_candidates`), the files it does not load in
  `generations.json`, and records the YMAPs newer than the build with a successor as `newer_files`, so that
  sources streaming them are diffed against them.
- The merge carries each source's changes to the items they pair with in the loaded
  generation, and a source YBN's polygon changes to the loaded YBNs whose boxes overlap it,
  polygon by polygon by geometry, deploys only
  that generation, and reports the changes it does not apply or cannot carry.

The engine's loading rules are inferred from the packs' data and loads observed in game;
the config's `[dlc_activity]` corrects them where they do not hold.

## Merge

1. Ensure the derived vanilla and source caches are current.
2. Carry each source YMAP's changes, after its LOD light pair document's skip and its
   overwrite, over to the paired items of the newer generations the game loads, or of the
   older ones for a YMAP first shipped after the build (`app::map::merge::generations`),
   and each source YBN's changes of a generation not loaded to its successor, writing them
   to `generations/`.
3. Leave out the YMAPs with a successor, omitting their sources, and dispatch the other
   latest vanilla `.ybn` and `.ymap` files, and the YMAPs resources add, to separate merge
   handlers.
4. For YMAPs, read each source's diff document, leave out the additions its LOD light pair
   document skips and the differences its resource overwrite selects, add the LOD light
   removals of resources shipping only the other YMAP of a light pair, as their documents
   imply them and their overwrites keep them, and the changes carried over from other
   generations, newest origin first,
   append the vanilla overwrite as the last source, leave out the entity additions an
   earlier source makes already, matched as the versions of an added YMAP are below,
   combine the differences and apply them to the latest vanilla file. Combining keeps
   every change of its inputs; the merge stage first leaves name, flag, extent, block and
   distant LOD light count changes and physics dictionary removals out of each
   resource's diff, keeping vanilla's, but not out of the vanilla overwrite, through
   which they can still be set. Entities' child counts are left out of every diff,
   the vanilla overwrite's included: the merge counts each parent's children from the
   final parent references, and removes the parents it leaves without children, up the
   LOD hierarchy. Diffs and their application stay complete, so other uses
   replay every change. For YBNs, read each source's diff document, leave out the polygons
   its overwrite skips, add the changes carried over from other generations, combine them
   and apply them to the latest vanilla file, placing added polygons so connected ones share
   a geometry child (`common::mesh`), and leave out repeated polygons. The loaded vanilla
   YBNs the vanilla cache counts enough repeats in (`ybn_repeats.json`) are built without them
   though no resource changes them. A YBN one source replaces, without an overwrite, carried
   changes or repeats to leave out, is deployed as that source has it.
5. Write merged native stream files under extension subdirectories (`ymap/`, `ybn/`),
   `_omit.txt` and the merge report (`report/`, `app::map::merge::report`) at the selected
   output root.

A stream file that resources add, named after no vanilla file, is merged from every
resource's version that no overwrite ignores, as the overwrite cache records them, each
compared with an empty file of its format; YMAPs are the only format merged this way so far.
For an added YMAP, its name, flags, extents and block are kept. Versions are merged from the resource whose name sorts last, the one
loaded last, and the additions an earlier version makes already are left out before the
diffs are combined (`core::format::ymap::compare::added`): entities by a GUID unique to each
version or by type,
archetype, LOD level and position, other items by equality within the matching tolerances
(`struct_diff::Diff::same_as`), and the extents cover every version's. Parent references to
an entity left out in favour of one with another GUID resolve to the kept one through an
alias in the merge's parent references, wherever the child is. An added YMAP one resource ships is compared with that source
rather than the empty YMAP, so it is written only when the merge changes it and is
otherwise deployed as a clone; one several resources ship is always written, replacing
theirs.

Merge output is staged and replaces the selected output directory after a
successful run. The default output is `asset/map/merged`. A missing or stale diff
document fails the merge. YMAP output is limited to final models that differ from
vanilla after merging and parent-reference repairs. Only source YMAPs replaced by
generated output are listed in `_omit.txt`. Merged YMAPs are written
as RSC7 META with one schema table shared by every YMAP, taken from the latest
vanilla YMAPs (whose structures each have a single layout), embedding the schemas
their blocks use.

Merge records input provenance and output freshness. Timestamp-only changes
refresh provenance without remerging. YBN groups are independent basenames;
YMAP groups follow connected vanilla and source parent/child relationships so
related changes cannot reuse stale outputs. Diff documents, overwrite digests and
effective tolerances participate in group signatures. Missing or changed outputs
are rebuilt; removed inputs and results that become no-ops disappear from the next
publication. Failed merges leave prior outputs intact; fully current runs do not
republish. Algorithm versions are tracked separately per stream file type.

The merge report names, for each entity and item of another array that several changes
reach, the applied change and the ignored ones with their resolution reason, and the
changes of YMAPs not deployed that are carried nowhere.

## Overwrites

Overwrites are user edits applied to merge inputs without changing source or
cache bytes; besides the sources, they are the only input written from outside the tool.
The source cache preprocesses the sources alone. What the overwrites call for is derived into the overwrite cache, as the source cache is
from the sources, by its own stage before the merge: the resources' files each stream file
that resources add is merged from, in merge order (`added_stream_files.json`), and the
overwrites that
no longer match (`warnings.json`). The
cache records a digest of the source and vanilla caches and every overwrite, and a merge
reading one that no longer matches fails.

`overwrite/resources/{resourceName}/{stream_path}.diff.json` lists the differences of that
stream file's diff document to leave out of the merge, or leaves the file out whole.
Array items are selected by content rather than position. An entry leaving out
differences applies only while the source file and the diff document's baseline match the
fingerprints it was written against; otherwise it is skipped with a warning, as are
selectors that match no difference.
The overwrite cache checks every overwrite against the current files whenever its inputs
change and records those that do not apply in `warnings.json`; merge logs and returns them
on each run, including runs that reuse their output, and the pipeline prints them as
`ignored_overwrites`. Before that, a resource overwrite whose source alone changed is followed
to the new version, the source cache keeping the version it was written against by its
SHA-256 under `history/`: the overwrite cache compares that version's diff document with the
current one, rewrites the overwrite in the overwrites when its selectors carry over, and
records what it did under `source_updates/`, which, unlike the rest of the cache, is kept.

`overwrite/vanilla/{version}/ymap/{fileName}.diff.json` holds differences written
directly against the vanilla YMAP at `{version}/ymap/{fileName}` in the vanilla
archive. Merges compare sources with the latest vanilla files, so it applies only
while that file is the latest one of its name, selected by the game build, and
matches its fingerprint; once a newer stage replaces the file, it is left out with a
warning until it is written for the new version. The differences join the merge as
the last source rather than overriding its result, so the merge policy applies to
them unchanged; a file with one is merged even when no resource replaces it. Resource
overwrites apply to YBNs too; vanilla overwrites only to YMAPs.

## Inspection

### Model Reading

`--read` writes typed stream-file models as JSON under `asset/map/read-cache` for
external tools such as overwrite editors. Each document records the fingerprints
of its file and, for a YMAP, of the hash-name table used to name it, and is rewritten only when
those change or when forced. Resources are read from the source cache; vanilla
files from a version of the raw archive.

### Read-Only Searches

GUID, entity-position, YBN-center, occluder and file-from-position searches read
existing native files and return matches as XML.
Filename globs limit reads; position searches use an inclusive 3D radius. YBN
centers are resolved to world space, and indices are local to each input, not
persistent IDs.

The vanilla cache, source cache and merge output each keep a bounds index of their
stream files, `stream_file_rtree.json`, since they change at different rates. Each
step updates its store's index after building or reusing the store. The index lists
its files from the store's metadata (the vanilla cache, the source-cache inventory,
the merge output), records that metadata's fingerprint and is rebuilt only when it
changes or when the index is missing or outdated, so an interrupted step is repaired
by the next run. Each store module owns reading its metadata. Bounds are read without
decoding the rest of a file: a YMAP's entity extents, a YBN's bound tree without
geometry.

Optional pre-merge inspection reads recorded vanilla and source inputs, verifies
their fingerprints and marks absent matches. Changed inputs are rejected. The same predicate applies to every stage.

## Deploy

Deployment consumes existing merged output and the files the source-cache inventory
records as moved; a recorded file that is missing is an error. Merged stream files go to
`stream/{extension}/merged/{filename}`. Source-cache files not replaced by a
merged basename, other than the sources the merge omits without writing a file of
their name (a `cache_info.json` record with `superseded_by`, or a source marked
`made_from_older` of a file the merge leaves as vanilla), go to
`stream/{extension}/clone/{resourceName}/{stream-relative-path}`. Matching is
case-insensitive; archive and report files are excluded. Duplicate merged
basenames are rejected.

Deployment freshness is tracked independently of merge freshness. Unchanged
copies are skipped, and missing or altered outputs are repaired. Only managed
files are removed when stale, with a warning for modified ones; unrelated
files are preserved. Empty stream directories are pruned without following
symlinks. Force affects copying, not cache generation or merging.

The source omission list includes active moved merged/clone inputs. Original resource paths remain available for
inspection.

## Source Restoration

The explicit restore-source-cache operation returns recorded active cached files
to their original resource-relative paths under the selected source root. It
checks all paths and destination conflicts before moving files and preserves file
modification times.
Cache directories the restored files leave empty are removed, recreating source
directories as needed. Successful restoration invalidates source-cache indexes without modifying merge
or deployment output. Restoring selected resources keeps the inventory, recording
their files as in source again, so the other resources' cached files stay indexed;
only the derived reports are invalidated, which keeps merges from running on an
inventory whose conflicts and load plan describe the cached layout. The next source-cache generation inventories the restored
resources and applies normal extraction again. Cache generation and restoration
must not run concurrently.

Source-cache exclusions are explicit per-execution inputs, not saved settings.
Absent or empty exclusions mean no filtering. Pipeline and merge forward the
current exclusions to prerequisite source-cache construction. Excluded resources
remain in source and are omitted from indexing and conflict scans. Existing
excluded resources are removed from active indexes and their cache directories
are moved to history with the previous inventory retained for provenance. They no
longer contribute merge inputs or deployed clone files. Running without exclusions
does not automatically restore archived files.

## Merge Policy

Sources are compared with the latest selected vanilla files. Each stream file type has
policies of its own.

The outcome never depends on processing or filesystem order: a removal wins over
every other change to the same vanilla item, and otherwise the first source in
source order wins. A YMAP's own sources come before changes carried to it from other
generations, which come newest origin generation first. YMAP parent
relationships are resolved before merging and repaired after the final entity layout
is known, so files affected by those repairs are rebuilt even when their own data is
unchanged; ambiguous references are rejected. YBN shapes are
matched by geometry within the configured tolerance, not by polygon or vertex index, and
grouped by the collision flags of the geometry child holding them.
The exact rules are in the [command reference](reference.md#merge).

Matching tolerances are validated from configuration before each operation and
kept constant throughout it. Effective values participate in diff-document and
merge freshness so configuration changes cannot reuse results computed under
another matching policy.

## Vanilla Archive And Derived Cache

The raw vanilla archive preserves ordered overlays from the base game, title update and
DLCs, stored by version and extension, with a global hash-name table. It holds one or
both content sets, extracted in one pass over the archives: `map` (YMAP, YBN) and
`clothing` (YMT), listed in the manifest's `contents`; regeneration extracts the
requested content with the content the archive already holds. Each feature reads the
archive through its content's view (`core/game_archive::load_content`): every stage,
with only that content's files, as a game build loads them, and a revision over the
content's files of every build, so derived caches follow changes to their own content
only. The
table names hashes from YMAP META strings and archive entry names; entity archetypes
that neither spells out, mostly LOD and SLOD models, take the names CodeWalker derives
from `.ydr` and `.ydd` file names. A DLC
pack's stage reads its `dlc.rpf`, then the subpacks (`dlc1.rpf`, …) its `setup2.xml`
declares, as the game mounts them, and records the pack's device, data files and change
sets. The title update's patch of the pack (`update/update.rpf/dlc_patch/{pack}/`)
differs between game builds, so the stage keeps one version of it per distinct content, in
`{index}-{pack}/update_rpf/{build}/`, each recording the builds that ship it and what the
pack declares with it; a file two versions share is stored once. The versions are read
from FiveM's copies of each build's `update.rpf` in its game-storage, found by the SHA-1
the `[update_rpf]` configuration table lists per build, a build without a copy taking the
closest build's below it; without game-storage, only the installed game's patches are read.
The pack's versions of the files a patch replaces are kept beside it. A reader views the
stages as one game build loads them (`VanillaCacheManifest::for_build`): each pack's stage
followed by the build's version of the patch as a stage of its own, the packs after the
build's last stage taking the installed game's. The derived cache selects the latest files
through the last stage of the chosen game build, in that view, records the build, and
indexes YMAP parent/child relationships. Its metadata names the raw archive, relative to the cache
when they share a root so that the two move together, and each latest file's path in it;
every reader resolves the files through that metadata. A game build is placed among the stages by the `[gamebuild]`
table in configuration, which names the DLCs each FiveM build first includes. It can be
recreated from the raw archive without re-extracting the game. An existing raw archive is
not checked against changes to the installed game.

The raw vanilla archive lists every stage in `cache_info.json`, including stages without
file changes. Stage directories and extension subdirectories are created only when
they contain extracted additions or replacements; populated stages also hold
`version_info.json`.
The archive's root has one empty `latest_version-{version_id}` marker naming the
final listed stage, even when it has no file changes.

Generation of the following YMAP version diffs is currently disabled, together with
vanilla version inference (see [Limitations](reference.md#limitations)). When enabled, the derived cache also
writes, for each YMAP a version after its first changes, its
diff from the version before (`diff/{version}/ymap/{name}.diff.json`), and lists each
such YMAP's versions in `ymap_versions.json`. These diffs record every change: no
value tolerance, entity parents as raw indices, entity moves as removals and
re-additions, grass batches that instance changes would not rebuild replaced whole,
and any other array whose diff would not rebuild it rewritten. Each diff is checked
by applying it when taken, so applying them in order to a YMAP's oldest file
rebuilds every later version. Source diffs keep their tolerances and do not record
moves, which merging would take for deletions. A rebuild copies the
previous cache's diffs between unchanged files while the diff layout, algorithm
version and hash names match; a forced rebuild takes them all again.

Input fingerprints and upstream state drive reuse. Raw vanilla and merge outputs
are staged before publication. Source-cache file moves are incremental, so the whole
pipeline is not one transaction.

## Conversion and Dependencies

Binary/XML conversion, semantic comparison and in-game behavior provide different
guarantees. META rebuilding requires compatible schemas; PSO editing uses an
existing binary template and cannot change allocated shapes or capacities.
The CodeWalker round-trip tests require locally supplied assemblies, and RPF decryption a locally
supplied `magic.dat`. Unsupported required data fails explicitly.

TypeScript consumers use declarations generated at release time for persisted
reports, diff documents, overwrites, read documents and configuration inputs,
derived from the serialization models. They provide no runtime JSON validation or
lossless handling of large JavaScript numbers.

## Boundaries

- The installed-game cache is an archive-overlay history, not a reconstruction of previous game releases.
- Full mount enable/disable rules and engine-level deletions are not modeled.
- Parsed-model equality does not guarantee byte identity with an original resource.
- Source diff documents describe differences from one vanilla file; they are not lossless patches. Vanilla version diffs are.
- Conversion tests do not replace runtime validation in GTA V/FiveM.
- FiveM preview and editing are external to this tool.

Usage: [Readme](../Readme.md), [command reference](reference.md).
Remaining work: [TODO.md](TODO.md).

## Clothing Collection

Clothing YMTs are the vanilla archive's `clothing` content
(`--generate-vanilla --content clothing`). Its selector, in `core/game_archive`,
keeps raw `CPedVariationInfo` and `CCreatureMetaData` YMTs by root type, and their
`ped_variation` or `creature_metadata` kind is recorded per changed file in
`cache_info.json`. Same-name overlays and unchanged-file reuse follow the map content.

A derived counts cache (`--clothing --generate-vanilla-cache`, owned by
`clothing/cache`) reads each stage's changed YMTs through the shared structure
readers, whichever container holds them, and counts only `ped_variation` files,
recording drawables and textures per component slot and props per anchor, with
per-stage summaries. Creature metadata stays available in the raw archive but is
not part of clothing counts. The cache is tied to the revision of the archive's
clothing content and rebuilt when that changes or on force.
