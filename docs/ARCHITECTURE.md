# Architecture

## Overview

MLO Merger prepares vanilla data, merges supported FiveM map and collision
changes, and deploys the resulting stream files. Inspection and conversion are
separate operations that do not run the pipeline.

Native Rust handles game-file conversion and merging. CodeWalker is an optional
conversion backend and supplies RPF reading for vanilla cache generation.

## Pipeline

The default command runs vanilla extraction when no raw archive exists, then
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

Each stage retains its own freshness and prerequisite checks. Pipeline force
applies to derived stages, not existing raw vanilla. Explicit stage selection
and a pipeline-wide gamebuild ceiling remain unimplemented.

## Merge

1. Ensure the derived vanilla and source inventory caches are current.
2. Enumerate files under `vanilla-cache/latest` and dispatch `.ybn` and `.ymap` files to separate merge handlers.
3. Match source files by basename and apply their supported changes directly to each latest vanilla file.
4. Write merged native stream files under extension subdirectories (`ymap/`, `ybn/`) and `_omit.txt` at the selected output root.

Merge output is staged and replaces the selected output directory after a
successful run. The default output is `asset/merged`. Its source-cache
prerequisite moves vanilla-named source stream files into the cache; merge reads
those cached files and retains original resource paths for omit information.
YMAP output is limited to final models that differ from vanilla after supported
merges and parent-reference repairs. Identical inputs and basename conflicts
alone do not cause output; changed fields, runtime entity order and repaired
references do. A run with no edited YMAPs succeeds without generating YMAP files.
Only source YMAPs replaced by generated output are listed in `_omit.txt`.

Merge records input provenance and output freshness. Timestamp-only changes
refresh provenance without remerging. YBN groups are independent basenames;
YMAP groups follow connected vanilla and source parent/child relationships so
related changes cannot reuse stale outputs. Missing or changed outputs are rebuilt.
Removed inputs and results that become no-ops disappear from the next publication.
Failed merges leave prior outputs intact; fully current runs do not republish.
Algorithm versions are tracked separately for YMAP and YBN; changing one format's
merge behavior invalidates only that format's cached groups.

YMAP duplicate diagnostics identify applied and ignored source edits with their
resolution reason. They describe competing entity changes, not filename conflicts.

## Inspection

### Resource Light Editing

On-demand light JSON exports resolve source and moved-cache paths to the original
resource identity. Only resource-local LOD/distant relationships are followed;
light metadata indexing avoids loading unrelated entity hierarchies. Linked array
occurrences, dependency fingerprints and conservative influence OBB/AABBs let an
external editor select lights without repeated relationship or geometry work.
Per-document sidecars reuse unchanged exports and repair missing or altered JSON.
The stream R-tree uses the original file bounds independently of light editing.
Light influence bounds are computed only for on-demand light exports and edits.

Typed removal documents under the overwrite directory are validated against the
exported source snapshot. The active source-cache inventory determines resource
and file membership; overwrite does not discover live stream directories or
reinterpret exclusions. Source-cache APIs own recorded source/cached path
resolution, stream-byte reads, fingerprints and preparation-cache I/O. Native
light decoding and edit composition remain in the light module. Editing withdraws occurrences from a resource's light
contribution, not from every resource sharing a basename. Edited groups compose
linked light pairs while the normal merge continues to own non-light data.
Resource providers retain exact duplicate multiplicity, and parent/child arrays
are emitted in the same final order. Unsupported or stale changes fail before
publication; original source/cache bytes are never overwritten.

Validated overwrite preparation is cached independently in source-cache. Input
mtime and size permit fingerprint reuse; diff membership, resource paths and
source content determine whether prepared light models can be reused. Force
bypasses preparation reuse. The cache is committed only after successful merge
publication or a successful no-op run; failed validation does not update it.
Input verification and group signatures reuse the same checked fingerprints.

Diff contents and participating inputs affect the YMAP group signature. Custom
light maps without vanilla are supported when their non-light data does not
conflict. Merge records original paths replaced by these outputs so deployment
can include unmoved custom files in its omission list. Withdrawing edits restores
ordinary merge/clone behavior. Generic global post-merge overwrites remain a
separate future workflow. FiveM rendering and editing are external to this tool.

### Read-Only Searches

GUID, entity-position and YBN-center searches read existing native files and
return all matching occurrences as XML, without writing artifacts. Filename
globs limit reads; position searches use an inclusive 3D radius. YBN centers are
resolved to world space, and indices are local to each input, not persistent IDs.

Optional pre-merge inspection reads recorded vanilla and source inputs, verifies
their fingerprints and marks absent matches. Changed inputs are rejected rather
than presented as historical data. The same predicate applies to every stage.

## Deploy

Deployment consumes existing merged output and `source-cache/resources` without
regenerating either input. Merged stream files go to
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

The source omission list includes active moved merged/clone inputs and original
custom files superseded by light edits, not archived history or untouched source
files. Original resource paths remain available for inspection.

## Source Restoration

The explicit restore-source-cache operation returns recorded active cached files
to their original resource-relative paths under the selected source root. It
checks all paths and destination conflicts before moving files, preserves file
modification times, and does not restore archived history or unrecorded files.
Successful restoration invalidates source-cache indexes without modifying merge
or deployment output. The next source-cache generation inventories the restored
resources and applies normal extraction again. Cache generation and restoration
must not run concurrently.

Source-cache exclusions are explicit per-execution inputs, not saved settings.
Absent or empty exclusions mean no filtering. Pipeline and merge forward the
current exclusions to prerequisite source-cache construction; additional light
edits use its recorded resource/file membership and invalidate preparation when that changes.
Legacy exclusions files are ignored and removed after successful cache updates.
Excluded resources remain in source and are omitted from indexing and conflict
scans. Existing excluded resources are removed from active indexes and their cache
directories are moved to history with the previous inventory retained for provenance.
They no longer contribute merge inputs or deployed clone files. Source files are
untouched; running without exclusions does not automatically restore archived files.

## Merge Policy

YMAP merging treats vanilla entities omitted by a mod as deletions. Any deletion
for an entity takes precedence over additions, modifications or retention by
other resources, regardless of processing order. Parent relationships
are resolved before merging and repaired after the final entity layout is known.
Files affected by those repairs are rebuilt even when their own mod data is
otherwise unchanged. Ambiguous references are rejected rather than guessed.

Sources are compared directly with the latest selected vanilla files. Historical
baseline inference is not part of the default merge workflow. YMAP and YBN use
independent format-specific policies rather than a shared generic diff.

For YBNs, baseline-matched shapes missing from any source are removed and new
shapes are combined. Deletion takes precedence over material or triangle-corner
flag changes to the same geometry, regardless of resource order. Boxes are matched
by their eight corners so alternate native reference vertices do not resurrect
deleted volumes. Matching uses configured coordinate tolerance, not polygon or
vertex indices; geometrically distinct additions remain independent.
Source groups without a vanilla baseline are skipped or rejected when conflicting.

Matching tolerances are validated from configuration before each operation and
kept constant throughout it. Effective values participate in merge freshness so
configuration changes cannot reuse results computed under another matching policy.
YMAP's exact entity comparisons and format-specific identity rules remain separate
from configurable approximate comparisons and inspection search radii.

## Vanilla Archive And Derived Cache

The raw vanilla archive preserves ordered YMAP/YBN overlays from the base game,
title update and DLCs. The derived cache selects the latest files through the
chosen stage and indexes YMAP parent/child relationships. It can be recreated
from the raw archive without re-extracting the game.

Source-cache generation moves vanilla-named inputs from resources into the cache,
retaining original paths. Unmatched files stay in the resource; replacements
archive prior cached files. Resource names must be unique, and colliding stream
paths are rejected. Scoped updates retain other inventories for global conflict
and relationship analysis. Missing cached inputs are errors, not silent deletions.

Input fingerprints and upstream state drive reuse. Raw vanilla and merge outputs
are staged before publication; failed builds do not publish incomplete results.
Source-cache file moves are incremental, so the whole pipeline is not one
transaction. Version lookup reads overlay metadata without modifying artifacts.

## Conversion and Dependencies

Binary/XML conversion, semantic comparison and in-game behavior provide different
guarantees. META rebuilding requires compatible schemas; PSO editing uses an
existing binary template and cannot change allocated shapes or capacities.
CodeWalker requires locally supplied assemblies. Unsupported required data must
fail explicitly rather than be silently discarded.

TypeScript consumers use declarations generated at release time for persisted
reports and configuration inputs, derived from the serialization models rather
than separate handwritten contracts. Generation is independent of the asset
pipeline and does not provide runtime JSON validation or lossless handling of
large JavaScript numbers.

## Boundaries

- The installed-game cache is an archive-overlay history, not a reconstruction of previous game releases.
- Full mount enable/disable rules and engine-level deletions are not modeled.
- Parsed-model equality does not guarantee byte identity with an original resource.
- Semantic reports are not lossless snapshots or history patches.
- Conversion tests do not replace runtime validation in GTA V/FiveM.

Usage: [English](../Readme.md) / [Japanese](Readme.ja.md).
Remaining work: [TODO.md](TODO.md).