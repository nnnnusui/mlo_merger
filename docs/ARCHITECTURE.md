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

YMAP duplicate diagnostics identify applied and ignored source edits with their
resolution reason. They describe competing entity changes, not filename conflicts.

## Inspection

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

The moved-source list includes active merged and clone inputs, not unmoved source
files or archived history. Original resource paths remain available for inspection.

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
shapes are combined. Matching uses shape, material and coordinate tolerance, not
polygon or vertex indices. A geometrically different shape from another resource
can therefore remain as an addition even where a baseline shape was deleted.
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

## Boundaries

- The installed-game cache is an archive-overlay history, not a reconstruction of previous game releases.
- Full mount enable/disable rules and engine-level deletions are not modeled.
- Parsed-model equality does not guarantee byte identity with an original resource.
- Semantic reports are not lossless snapshots or history patches.
- Conversion tests do not replace runtime validation in GTA V/FiveM.

Usage: [English](../Readme.md) / [Japanese](Readme.ja.md).
Remaining work: [TODO.md](TODO.md).