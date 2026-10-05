# Architecture

## Overview

The application provides three related workflows:

- Merge mod map and collision resources against vanilla data.
- Cache the installed game's vanilla resources in archive-overlay order.
- Infer an MLO resource's vanilla baseline and export its differences.

Native Rust handles game-file conversion and merging. CodeWalker is an optional
conversion backend and supplies RPF reading for vanilla cache generation.

## Merge Pipeline

1. Discover FiveM resources and extract relevant mod YMAPs.
2. Convert YMAPs to XML against the available resource schemas.
3. Compare each mod with vanilla and combine changes.
4. Preserve unchanged binary clones or rebuild affected YMAPs.
5. Merge same-named YBN collisions against vanilla.
6. Optionally deploy generated resources and the source-file omit list.

Generated output is replaced for each pipeline run after confirmation. Vanilla
and source resources remain inputs; deployment replaces the selected output
resource's generated contents.

## Merge Policy

YMAP merging preserves vanilla entities omitted by a mod. Parent relationships
are resolved before merging and repaired after the final entity layout is known.
Files affected by those repairs are rebuilt even when their own mod data is
otherwise unchanged. Ambiguous references are rejected rather than guessed.

YBN merging requires a same-named vanilla baseline. Supported collision shapes
are compared geometrically rather than by binary table order. Removals and
additions are combined; unsupported shapes use conservative handling.

Structural YBN differences are available separately from binary history
patches. They describe primitive occurrences and Bounds metadata, but are not
yet an apply/merge API or an MLO YBN diff-cache workflow.

## Vanilla History

The vanilla cache applies base archives, the title update and listed DLCs in
order. DLC title-update patches are applied with their corresponding DLC.
Each stage records provenance, metadata and generation logs.

New files are stored directly; changed files are stored as deltas and unchanged
content is reused. YMAP deltas reconstruct parsed models, while YBN deltas
reconstruct exact binary content. These history formats are distinct from
merge-oriented semantic differences.

Cache publication occurs after all stages complete. Failures retain diagnostic
logs without publishing an incomplete cache. Version lookup is a read-only
metadata query over stages that introduced or changed a filename.

## MLO Baseline Selection

Each resource is processed independently. Its vanilla-matched YMAPs are compared
with the recorded content versions. Ties prefer the newer changed version; the
latest of the per-file best versions becomes the resource's shared baseline.
All final differences are then calculated against that baseline's cumulative
state.

Output includes differences, selection information, timestamps and logs.
Unmatched and unsupported stream files are recorded explicitly. The current
workflow compares YMAPs only.

## Conversion and Dependencies

The Native backend reads resource data, converts supported families to XML and
rebuilds binaries using their schemas. PSO rebuilding edits an existing binary
template rather than creating an arbitrary new resource. CodeWalker runs in
the application process and requires locally supplied assemblies.

XML conversion, history reconstruction and semantic comparison have different
guarantees. Stable repeated conversion is checked independently of parity with
CodeWalker and in-game behavior. Some resource families and unknown fields
remain unsupported; failures should be explicit rather than silently dropping
required data.

## Boundaries

- The installed-game cache is an archive-overlay history, not a reconstruction of previous game releases.
- Full mount enable/disable rules and engine-level deletions are not modeled.
- Parsed-model equality does not guarantee byte identity with an original resource.
- Conversion tests do not replace runtime validation in GTA V/FiveM.

Command usage is in [../Readme.md](../Readme.md); remaining work is tracked in
[TODO.md](TODO.md).