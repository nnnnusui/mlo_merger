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
6. Deploy the generated resource and source-file omit list.

Generated output under `.output/` is replaced for each pipeline run after
confirmation. The default deploy target is `.output/merged_mlo`; source resources
remain inputs and are never modified.

## Merge Policy

YMAP merging preserves vanilla entities omitted by a mod. Parent relationships
are resolved before merging and repaired after the final entity layout is known.
Files affected by those repairs are rebuilt even when their own mod data is
otherwise unchanged. Ambiguous references are rejected rather than guessed.

YMAP difference types and extraction live under `format/ymap/diff` and are
shared by merge and diff-cache generation. Pipeline merge reads raw mod files
and vanilla-cache history, tests changed vanilla versions newest-first, selects
the least-different baseline per file, then applies those changes to the latest
cached vanilla state. The generated source-cache is a separate reporting
artifact and is not a merge input.

YBN merging requires a same-named vanilla baseline. Supported collision shapes
are compared geometrically rather than by binary table order. Removals and
additions are combined; unsupported shapes use conservative handling.

YBN semantic differences are used by MLO diff-cache generation. They describe
primitive occurrences and Bounds metadata; conflict merge does not yet consume
these reports. Vanilla history stores raw native files instead.

## Vanilla Archive And Derived Cache

The raw vanilla archive (`asset/vanilla`) applies base archives, the title update
and listed DLCs in order. DLC title-update patches are applied with their
corresponding DLC. A build starts with an empty output directory; an optional
stage limit emits a complete prefix and avoids reading later DLC archives. Each
stage records provenance, metadata and generation logs. New or changed YMAP/YBN
files are stored as raw native files in that stage; unchanged files are omitted.
The manifest records the predecessor hash and source, and cumulative lookup
selects the most recent stored file for each name. This preserves source bytes
without replaying history
deltas. The manifest schema is currently version 1. Extraction also stores a
deduplicated set of extensionless RPF entry names in `rpf_names.json` for hash
resolution.

Raw vanilla generation also writes `hash_names.json`. Its single `names` map
combines embedded YMAP META strings, RPF entry-name candidates, and entity
archetype names as `hash -> text` entries. Entity GUIDs are not included. The
candidates do not reproduce CodeWalker's broader nametable or bundled-string
index.

The derived vanilla cache (`asset/vanilla-cache`) links the selected raw files
under `latest/ymap` and `latest/ybn`, and stores YMAP parent-to-child
relationships for the selected latest stage only. Its manifest records the raw
manifest fingerprint and timestamps of referenced raw files. A changed timestamp,
selected final stage, or missing link triggers a rebuild. This cache is separate
from the raw archive and can be recreated from it.

Raw archive publication occurs after all stages complete; failures retain
diagnostic logs without publishing an incomplete cache. Version lookup is a
read-only metadata query over stages that introduced or changed a filename.

## MLO Baseline Selection

Each resource is processed independently. Vanilla-matched files are compared
with changed content versions newest-first. Each file uses its lowest-difference
baseline, with ties favoring the newer version; merge then applies its changes to
the latest vanilla-cache state. Diff-cache generation records these per-file
comparisons for inspection, but pipeline merge reads source files directly.

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