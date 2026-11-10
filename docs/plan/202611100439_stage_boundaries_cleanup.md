# Stage Boundaries Cleanup

Tidy the boundaries between the stages (vanilla cache, source cache, overwrite cache, merge)
after the added-YMAP and overwrite work: remove duplicated logic, let each data owner provide its
reads, and split the files and functions that grew too large. Each step is one commit and leaves
every test passing.

## Decisions

- The merge requires an overwrite cache, built beforehand by `--generate-overwrite-cache` or a
  command that runs it (`--merge`, `--pipeline`). The merge no longer derives added-YMAP sources
  or overwrite warnings itself.
- What the merge leaves out of added YMAPs is recorded only in the overwrite cache
  (`added_ymaps.json`, `left_out`). `duplicates.json` no longer reports added YMAPs.

## Steps

### 1. One prerequisite chain in the CLI

- [x] Add `generate_overwrite_cache::update`, building the source cache and the overwrite cache
  as `generate_vanilla_cache::update` builds the vanilla cache.
- [x] Call it from `--merge` instead of repeating the chain.

### 2. Shared helpers

- [x] `MergeSourceFile::resource_name()` for the resource directory name, and
  `Overwrites::for_source(&MergeSourceFile)` for its overwrite, replacing the lookups in the
  merge, the incremental merge, the overwrite cache, its warnings and its source updates;
  `MergeInputs::ymap_by_key()` for finding an overwrite's source.
- [x] One stream-path key (`common::path`), replacing the three normalizations that differ on a
  leading `/`.
- [x] `From<&YmapComparisonFile> for OverwriteFingerprint`, and the empty baseline's fingerprint
  from the source cache, replacing the conversions written out in place.
- [x] `source_cache::diff_document_path`, replacing the `diff/...` paths built by hand,
  history's included.

### 3. The merge requires the overwrite cache

- [ ] `Merge::overwrite_cache_dir` and `Pipeline::overwrite_cache_dir` become required paths;
  remove `overwrite_cache::resolve_without_cache` and the merge's own warnings.
- [ ] The merge takes `AddedYmap` as the overwrite cache records it; remove `merge::added`'s
  copy of it.
- [ ] Update the tests that merge without an overwrite cache.

### 4. One record of what added YMAPs leave out

- [ ] Leave added YMAPs out of `duplicates.json`; their thinned additions are recorded in
  `added_ymaps.json` only.
- [ ] Document it in reference.md and usage.md.

### 5. Reads of the latest vanilla files from the vanilla cache

- [ ] `vanilla_cache::latest_stream_files`, listing the latest vanilla YMAPs and YBNs, replacing
  the merge's `dispatch_vanilla_stream_files` and the overwrite warnings' `latest_ymaps`.

### 6. Split modules that hold several concerns

- [ ] `source_cache/read.rs`: the read API (`load_merge_inputs`, `inventoried_files`,
  `cached_files`, ...) out of `source_cache/mod.rs`.
- [ ] `overwrite_cache/added.rs`: resolving added YMAPs and recording what they leave out, out of
  `overwrite_cache/mod.rs`.
- [ ] `web_sample/earlier.rs`: building the earlier versions, out of `web_sample/mod.rs`.

### 7. Split the YMAP merge engine

- [ ] Split `MergeYmap::run_configured_inner` (about 600 lines) into its phases: registering
  sources and parents, collecting each YMAP's diffs, combining them, relinking children, and
  deciding the output.
- [ ] One helper for recording a diff's entity changes in the duplicate report, replacing the
  three copies.

## Out of Scope

- `format/ybn/diff/legacy.rs`: replaced once YBNs get diff documents (see TODO.md).
- Following overwrites to a new vanilla version (see TODO.md).
