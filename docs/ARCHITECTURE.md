# Architecture

## Overview

MLO Merger prepares vanilla data, merges supported FiveM map and collision
changes, and deploys the resulting stream files. Inspection and conversion are
separate operations that do not run the pipeline.

Native Rust handles game-file conversion, merging and RPF reading for vanilla
generation. CodeWalker is used only by round-trip tests that compare conversion
with it, behind the `codewalker` feature; the released executable does not load it.

## Pipeline

`--pipeline` runs vanilla extraction when no raw archive exists, then
vanilla-cache, source-cache, merge and deploy in order. Existing raw vanilla is
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
as the source root.

Each stage retains its own freshness and prerequisite checks and writes the JSON
records the next stage needs. A single stage runs through its own operation
(`--generate-vanilla`, `--generate-vanilla-cache`, `--generate-source-cache`,
`--merge`, `--deploy`), which brings its prerequisites up to date first.
Pipeline force applies to derived stages, not existing raw vanilla. The game build
applies only to the vanilla-cache stage; raw vanilla always holds every stage.

## Module Responsibilities

Each artifact has one owning module (`vanilla`, `vanilla_cache`, `source_cache`,
`overwrite`, merge and deploy). The owner reads its own files, checks their
freshness and exposes the data other stages need; consumers do not walk another
stage's directories or parse its records. Overwrite application is likewise
exposed by the overwrite module, and merge reads overwrite data only through it.
The vanilla archive, vanilla cache, source cache and merge output each record their
metadata in `cache_info.json`.
Format codecs (`format/*`) stay independent of artifact layout and batch processing.
The GUI, opened when the executable starts without arguments, is a front end for
vanilla generation only: it collects the game directory and `magic.dat` and runs the
same generation as `--generate-vanilla`, holding no generation logic of its own.

## Source Cache

Source-cache generation moves vanilla-named inputs from resources into the cache,
retaining original paths. Unmatched files stay in the resource; replacements
archive prior cached files. Resource names must be unique, and colliding stream
paths are rejected. Scoped updates retain other inventories for global conflict
and relationship analysis. Missing cached inputs are errors, not silent deletions.

For each cached YMAP that overrides a vanilla YMAP, the source cache writes a diff
document against the latest vanilla file. The document records both files'
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

## Merge

1. Ensure the derived vanilla and source caches are current.
2. Dispatch latest vanilla `.ybn` and `.ymap` files to separate merge handlers.
3. For YMAPs, read each source's diff document, leave out the differences its
   resource overwrite selects, add the LOD light removals of resources shipping only
   the other YMAP of a light pair, append the vanilla overwrite as the last source,
   combine the differences and apply them to the latest vanilla file. Combining keeps
   every change unless told to leave some out; the merge stage leaves out name, flag,
   extent, block and distant LOD light count changes and physics dictionary removals,
   keeping vanilla's. Diffs and their application stay complete, so other uses replay
   every change. YBNs apply supported source changes directly.
4. Write merged native stream files under extension subdirectories (`ymap/`, `ybn/`),
   `_omit.txt` and `duplicates.json` at the selected output root.

Merge output is staged and replaces the selected output directory after a
successful run. The default output is `asset/merged`. A missing or stale diff
document fails the merge rather than being recomputed silently. YMAP output is
limited to final models that differ from vanilla after merging and parent-reference
repairs. Identical inputs and basename conflicts alone do not cause output. A run
with no edited YMAPs succeeds without generating YMAP files. Only source YMAPs
replaced by generated output are listed in `_omit.txt`. Merged YMAPs are written
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
republish. Algorithm versions are tracked separately for YMAP and YBN.

YMAP duplicate diagnostics identify applied and ignored source edits with their
resolution reason. They describe competing entity changes, not filename conflicts.

## Overwrites

Overwrites are user edits applied to merge inputs without changing source or
cache bytes. `overwrite/resources/{resourceName}.json` lists, per stream file of
that resource, the differences of its diff document to leave out of the merge.
Array items are selected by content rather than position. An entry applies only
while the source and vanilla files match the fingerprints it was written against;
otherwise it is skipped with a warning, as are selectors that match no difference.
Merge checks every overwrite against the current files on each run, including runs
that reuse their output, and returns those it leaves out; the pipeline prints them as
`ignored_overwrites`.

`overwrite/vanilla/latest/ymap/{fileName}.diff.json` holds differences written
directly against a latest vanilla YMAP. Only `latest` is read: merges compare
sources with the latest vanilla files, and recent game updates rarely replace
existing YMAPs. The differences join the merge as the last source rather than
overriding its result, so the merge policy applies to them unchanged; a file with
one is merged even when no resource replaces it. It applies only while the latest
vanilla file matches its fingerprint. YBN overwrites are not yet supported.

## Inspection

### Model Reading

`--read` writes typed stream-file models as JSON under `asset/read-cache` for
external tools such as overwrite editors. Each document records the fingerprints
of its file and of the hash-name table used to name it, and is rewritten only when
those change or when forced. Resources are read from the source cache; vanilla
files from a version of the raw archive.

### Read-Only Searches

GUID, entity-position, YBN-center, occluder and file-from-position searches read
existing native files and return matches as XML, without writing artifacts.
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
their fingerprints and marks absent matches. Changed inputs are rejected rather
than presented as historical data. The same predicate applies to every stage.

## Deploy

Deployment consumes existing merged output and the files the source-cache inventory
records as moved, without regenerating either input; other files under
`source-cache/resources` are not part of the cache and are not deployed, and a recorded
file that is missing is an error. Merged stream files go to
`stream/{extension}/merged/{filename}`. Source-cache files not replaced by a
merged basename go to
`stream/{extension}/clone/{resourceName}/{stream-relative-path}`. Matching is
case-insensitive; archive and report files are excluded. Duplicate merged
basenames are rejected rather than selected arbitrarily.

Deployment freshness is tracked independently of merge freshness. Unchanged
copies are skipped, and missing or altered outputs are repaired. Only managed
files are removed when stale; modified stale files are rejected and unrelated
files are preserved. Empty stream directories are pruned without following
symlinks. Force affects copying, not cache generation or merging.

The source omission list includes active moved merged/clone inputs, not archived
history or untouched source files. Original resource paths remain available for
inspection.

## Source Restoration

The explicit restore-source-cache operation returns recorded active cached files
to their original resource-relative paths under the selected source root. It
checks all paths and destination conflicts before moving files, preserves file
modification times, and does not restore archived history or unrecorded files.
Successful restoration invalidates source-cache indexes without modifying merge
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

Sources are compared with the latest selected vanilla files. Historical baseline
inference is not part of the merge workflow. YMAP and YBN use independent
format-specific policies.

The outcome never depends on processing or filesystem order: a removal wins over
every other change to the same vanilla item, and otherwise the first source in
source order wins. Configured resource priority is not implemented. YMAP parent
relationships are resolved before merging and repaired after the final entity layout
is known, so files affected by those repairs are rebuilt even when their own data is
unchanged; ambiguous references are rejected rather than guessed. YBN shapes are
matched by geometry within the configured tolerance, not by polygon or vertex index.
The exact rules are in the [command reference](reference.md#merge).

Matching tolerances are validated from configuration before each operation and
kept constant throughout it. Effective values participate in diff-document and
merge freshness so configuration changes cannot reuse results computed under
another matching policy. They do not change search radii.

## Vanilla Archive And Derived Cache

The raw vanilla archive preserves ordered YMAP/YBN overlays from the base game,
title update and DLCs, stored by version and extension, with a global hash-name
table. The derived cache selects the latest files through the last stage of the
chosen game build and indexes YMAP parent/child relationships. A game build is placed
among the stages by the `[gamebuild]` table in configuration, which names the DLCs
each FiveM build first includes. It can be recreated from the raw archive
without re-extracting the game. An existing raw archive is not checked against
changes to the installed game.

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
are staged before publication; failed builds do not publish incomplete results.
Source-cache file moves are incremental, so the whole pipeline is not one
transaction. Version lookup reads overlay metadata without modifying artifacts.

## Conversion and Dependencies

Binary/XML conversion, semantic comparison and in-game behavior provide different
guarantees. META rebuilding requires compatible schemas; PSO editing uses an
existing binary template and cannot change allocated shapes or capacities.
The CodeWalker round-trip tests require locally supplied assemblies, and RPF decryption a locally
supplied `magic.dat`. Unsupported required data must
fail explicitly rather than be silently discarded.

TypeScript consumers use declarations generated at release time for persisted
reports, diff documents, overwrites, read documents and configuration inputs,
derived from the serialization models rather than separate handwritten contracts.
Generation is independent of the asset pipeline and does not provide runtime JSON
validation or lossless handling of large JavaScript numbers.

## Boundaries

- The installed-game cache is an archive-overlay history, not a reconstruction of previous game releases.
- Full mount enable/disable rules and engine-level deletions are not modeled.
- Parsed-model equality does not guarantee byte identity with an original resource.
- Source diff documents describe differences from one vanilla file; they are not lossless patches. Vanilla version diffs are.
- Conversion tests do not replace runtime validation in GTA V/FiveM.
- FiveM preview and editing are external to this tool.

Usage: [Readme](../Readme.md), [command reference](reference.md).
Remaining work: [TODO.md](TODO.md).
