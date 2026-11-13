# Architecture

## Overview

MLO Merger prepares vanilla data, merges supported FiveM map and collision
changes, and deploys the resulting stream files. Inspection and conversion are
separate operations.

Native Rust handles game-file conversion, merging and RPF reading for vanilla
generation. CodeWalker is used only by round-trip tests that compare conversion
with it, behind the `codewalker` feature.

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
records the next stage needs. A single stage runs through its own operation
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
metadata in `cache_info.json`. Recorded paths are relative to the store holding each file
(resources and their files to the source root), and freshness compares contents, with
modification times only as a hint to skip hashing. Where the stores are, and how a command
names them, does not affect any stage. The source cache identifies its source tree by its
resources rather than its path: a source root holding none of them is rejected.
Format codecs (`format/*`) stay independent of artifact layout and batch processing.
The GUI, opened when the executable starts without arguments, is a front end for
vanilla generation only: it collects the game directory and `magic.dat` and runs the
same generation as `--generate-vanilla`.

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

DLCs ship vanilla YMAPs again under prefixes of their own, and the game loads every
generation. The vanilla cache pairs the items of each family's generations, one unit per
older and newer generation, stored by a digest of its YMAPs so that rebuilds and game build
switches reuse them, and indexes the selected build's units in `generations.json`. After
taking the diffs, the source cache carries each resource's removals and modifications of
vanilla items over to the paired items of newer generations it does not ship, as
`propagated/` documents whose items name their origin in `reason`; they are derived from the
diff record and the generation index alone, and are kept while both are unchanged.

## Merge

1. Ensure the derived vanilla and source caches are current.
2. Dispatch latest vanilla `.ybn` and `.ymap` files, and the YMAPs resources add, to
   separate merge handlers.
3. For YMAPs, read each source's diff document, leave out the differences its
   resource overwrite selects, add the LOD light removals of resources shipping only
   the other YMAP of a light pair and the changes carried over from older generations,
   append the vanilla overwrite as the last source,
   combine the differences and apply them to the latest vanilla file. Combining keeps
   every change of its inputs; the merge stage first leaves name, flag, extent, block and
   distant LOD light count changes and physics dictionary removals out of each
   resource's diff, keeping vanilla's, but not out of the vanilla overwrite, through
   which they can still be set. Entities' child counts are left out of every diff,
   the vanilla overwrite's included: the merge counts each parent's children from the
   final parent references. Diffs and their application stay complete, so other uses
   replay every change. YBNs apply supported source changes directly.
4. Write merged native stream files under extension subdirectories (`ymap/`, `ybn/`),
   `_omit.txt` and `duplicates.json` at the selected output root.

A stream file that resources add, named after no vanilla file, is merged from every
resource's version that no overwrite ignores, as the overwrite cache records them, each
compared with an empty file of its format; YMAPs are the only format merged this way so far.
For an added YMAP, its name, flags, extents and block are kept. Versions are merged from the resource whose name sorts last, the one
loaded last, and the additions an earlier version makes already are left out before the
diffs are combined (`core::format::ymap::compare::added`): entities by GUID or by type,
archetype, LOD level and position, other items by equality within the matching tolerances
(`struct_diff::Diff::same_as`), and the extents cover every version's. Parent references to
an entity left out in favour of one with another GUID resolve to the kept one through an
alias in the merge's parent references, wherever the child is. An added YMAP one resource ships is compared with that source
rather than the empty YMAP, so it is written only when the merge changes it and is
otherwise deployed as a clone; one several resources ship is always written, replacing
theirs.

Merge output is staged and replaces the selected output directory after a
successful run. The default output is `asset/merged`. A missing or stale diff
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
republish. Algorithm versions are tracked separately for YMAP and YBN.

YMAP duplicate diagnostics identify applied and ignored source edits with their
resolution reason. They describe competing entity changes.

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
them unchanged; a file with one is merged even when no resource replaces it. YBN
overwrites are not yet supported.

## Inspection

### Model Reading

`--read` writes typed stream-file models as JSON under `asset/read-cache` for
external tools such as overwrite editors. Each document records the fingerprints
of its file and of the hash-name table used to name it, and is rewritten only when
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
merged basename go to
`stream/{extension}/clone/{resourceName}/{stream-relative-path}`. Matching is
case-insensitive; archive and report files are excluded. Duplicate merged
basenames are rejected.

Deployment freshness is tracked independently of merge freshness. Unchanged
copies are skipped, and missing or altered outputs are repaired. Only managed
files are removed when stale; modified stale files are rejected and unrelated
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

Sources are compared with the latest selected vanilla files. YMAP and YBN use
independent format-specific policies.

The outcome never depends on processing or filesystem order: a removal wins over
every other change to the same vanilla item, and otherwise the first source in
source order wins. YMAP parent
relationships are resolved before merging and repaired after the final entity layout
is known, so files affected by those repairs are rebuilt even when their own data is
unchanged; ambiguous references are rejected. YBN shapes are
matched by geometry within the configured tolerance, not by polygon or vertex index.
The exact rules are in the [command reference](reference.md#merge).

Matching tolerances are validated from configuration before each operation and
kept constant throughout it. Effective values participate in diff-document and
merge freshness so configuration changes cannot reuse results computed under
another matching policy.

## Vanilla Archive And Derived Cache

The raw vanilla archive preserves ordered YMAP/YBN overlays from the base game, title
update and DLCs, stored by version and extension, with a global hash-name table. A DLC
pack's stage reads its `dlc.rpf`, then the subpacks (`dlc1.rpf`, …) its `setup2.xml`
declares, as the game mounts them, then its title-update patch. The derived cache selects
the latest files through the last stage of the chosen game build and indexes YMAP
parent/child relationships. Its metadata names the raw archive, relative to the cache
when they share a root so that the two move together, and each latest file's path in it;
every reader resolves the files through that metadata. A game build is placed among the stages by the `[gamebuild]`
table in configuration, which names the DLCs each FiveM build first includes. It can be
recreated from the raw archive without re-extracting the game. An existing raw archive is
not checked against changes to the installed game.

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
