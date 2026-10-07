# Architecture

## Overview

The application provides three related workflows:

- Merge mod map and collision resources against vanilla data.
- Cache the installed game's vanilla resources in archive-overlay order.
- Infer an MLO resource's vanilla baseline and export its differences.

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

The CLI owns one logger for the entire pipeline. Each derived stage retains its
incremental freshness checks, including checks performed by prerequisite calls.
Pipeline force applies to derived stages, not existing raw vanilla. Explicit
stage selection and a pipeline-wide gamebuild ceiling remain unimplemented.

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

Merge records per-filename source and vanilla mtime, size and SHA-256, dependency
links, an algorithm version, and the output fingerprint or a no-output result
in `merge_cache_info.json`. Unchanged file stats reuse hashes; timestamp-only
changes refresh provenance without remerging. YBNs are independent basename
groups. YMAPs are conservative connected parent/child groups formed from both
vanilla relationships and source parent references, so a changed layout cannot
reuse stale related maps. Only groups with changed contents/dependencies or
missing/altered output are merged again. `-f` forces all current groups while
prerequisites retain their normal freshness checks.

Unchanged outputs are linked or copied into staging only when publication is
needed; their modification times are preserved. Removed inputs and results that
become no-ops disappear from the new publication. Failed runs leave existing
merged files and merge metadata intact. A fully current run does not replace
the merged directory.

## Inspection

Entity GUID and position-radius searches read existing merged native YMAPs without executing pipeline
stages or writing artifacts. Case-insensitive filename glob filtering limits
reads for both merged output and recorded pre-merge inputs, and the result is
one XML document containing all matching occurrences. Optional pre-merge output
uses recorded vanilla/source provenance from the merge cache, validates those
input fingerprints, and marks stages where the entity is absent. The command
does not claim historical data when recorded input files have changed.
Position searches use an inclusive 3D Euclidean radius, native coordinate
precision and the same predicate across merged, vanilla and source stages.

## Deploy

Deployment consumes existing merged output and `source-cache/resources` without
regenerating either input. Merged stream files go to
`stream/{extension}/merged/{filename}`. Source-cache files not replaced by a
merged basename go to
`stream/{extension}/clone/{resourceName}/{stream-relative-path}`. Matching is
case-insensitive; archive and report files are excluded. Duplicate merged
basenames are rejected rather than selected arbitrarily.

The deployment cache records both input and copied-file mtime, size and SHA-256.
Unchanged file stats reuse fingerprints; changed stats rehash the file. Content
matches skip copying, including timestamp-only changes. Missing or altered copies
are repaired. Files no longer selected are removed only when the cache owns them
and they have not been locally modified; unrelated files remain untouched.
Each copy is staged individually and preserves source mtime. Deployment metadata
is published after the file updates. `-f` forces copying without forcing merge or
source-cache generation. The default output is `asset/merged_mlo`.
Deployment's `files.txt` records original source-relative paths of active moved
cache files, including both merged inputs and clones. It is derived from
source-cache provenance, not from the merged omit list; unmoved and archived
files are excluded. Entries are sorted and deduplicated, and unchanged content
preserves the list's modification time.

## Merge Policy

YMAP merging preserves vanilla entities omitted by a mod. Parent relationships
are resolved before merging and repaired after the final entity layout is known.
Files affected by those repairs are rebuilt even when their own mod data is
otherwise unchanged. Ambiguous references are rejected rather than guessed.

YMAP difference types and extraction live under `format/ymap/diff` and are
shared by merge and diff-cache generation. Ver1 compares each source map
directly with the matching latest vanilla stream file, applies the supported
changes, repairs parent references, and rebuilds native files. Historical
baseline inference is planned for ver2. The source-cache is refreshed as a
prerequisite; merge decodes source and latest vanilla binaries directly into
YMAP models without an XML input round-trip. Edited models and their ordered
entities are written directly to RSC7 binaries using compatible embedded META
schemas from the selected vanilla/source files. Merge does not create intermediate
XML or schema-copy directories. Explicit XML conversion and legacy XML-input
merge workflows remain available separately.
Native YMAP and YBN writers share checked record writes and 16-byte aligned
storage, while META schemas and collision Bounds remain format-specific.

YBN merging applies source files directly to the matching latest vanilla file.
Multiple resources with the same basename are combined deterministically by
sorted source path; source files without a vanilla baseline are skipped unless
multiple resources collide on that name, which is an error. Version-aware
baseline selection is planned for a later version.

YBN semantic differences describe primitive occurrences and Bounds metadata.
Merge and MLO diff-cache use this model independently from YMAP merging.
Vanilla history stores raw native files instead of replaying semantic reports.

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

The source-cache command first ensures the derived vanilla cache is current,
then inventories supported YMAP/YBN inputs by resource and format. It records
source paths, fingerprints, exact latest-vanilla content matches, and
cross-resource basename conflicts; it does not generate semantic or binary
diffs. Changed source parents expand the YMAP read/rebuild plan through the
latest relationship index, including source children in other resources. Input
and per-resource upstream revisions allow unchanged resources to be skipped.
Vanilla-named YMAP/YBN files move from each resource's stream directories into
`source-cache/resources/{resourceName}/{stream-relative-path}`. The leading `stream/` or
`streams/` component is omitted from cached and archived paths, while metadata
retains original resource-relative paths for conflict and omit output. Recorded
legacy cached paths migrate when the resource is checked.
Legacy resource folders directly under the cache migrate under `resources/`;
the timestamped history layout under `_old` remains unchanged.
Colliding relative paths across stream roots are rejected rather than overwritten. Unmatched files stay in
the resource. Replacements move the previous file and fingerprint/mtime records
to `_old/{UTC timestamp}` with a snapshot of the prior cache metadata. Resource
names must be unique. Moving a resource between input groups without changing
its name preserves its cached files and fingerprints; only its source paths
and resource keys change. Content changes during a move follow normal update
and replacement rules. Ambiguous cached names are rejected rather than guessed.
Resource directories, manifests and stream presence are
tracked even when a resource has no stream files. An optional resource selection
limits checks and forced updates; global conflict and load plans still include
the retained inventories of other resources. Cache metadata is staged before
publication, while file moves are incremental and do not replace the cache tree.
The stream-conflict report's `conflicts` scans active resource directories in
the source cache, excluding `_old` and report files. Its paths are relative to
the source cache recorded as the input directory; its scan/conflict counts
describe that cached data.
`source_conflicts` preserves the original inventory's resource-relative conflict
paths, including moved files and files left in source resources. Each invocation
checks the cache report even when no resource inventory needs updating.
Merge consumes the
extension-specific conflict inventory and only vanilla-backed source files;
YMAP model decoding is limited to selected source maps and the recorded vanilla
parent/child closure. Fingerprinting first compares cached size and nanosecond
mtime, reusing SHA-256 when both are unchanged and hashing only new or stat-changed
files. Unchanged file contents reuse decoded inventory metadata. Directory,
manifest, stream presence, file stat/fingerprint or per-resource upstream changes
trigger an update. A missing moved file without an original source is an error,
not a reason to silently discard its inventory.

Raw archive publication occurs after all stages complete; failures retain
diagnostic logs without publishing an incomplete cache. Version lookup is a
read-only metadata query over stages that introduced or changed a filename.

## MLO Baseline Selection

Each resource is processed independently. Vanilla-matched files are compared
with changed content versions newest-first. Each file uses its lowest-difference
baseline, with ties favoring the newer version; merge then applies its changes to
the latest vanilla-cache state. Diff-cache generation records these per-file
comparisons for inspection, but merge reads source files directly.

Output includes differences, selection information, timestamps and logs.
Unmatched and unsupported stream files are recorded explicitly. The current
diff-cache workflow compares YMAPs only.

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