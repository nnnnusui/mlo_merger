# Stream File Generalization

Prepare the stages for formats beyond YMAP while keeping the current behavior: name what applies
to every stream file after stream files rather than YMAPs, and settle which files count as added.
YMAP-specific concepts (parent relationships, LOD light pairs, generations, the empty YMAP
baseline, `MergeYmap`, `YmapReadDocument`, ...) keep their YMAP names. Each step is one commit and
leaves every test passing.

## Decisions

- Stream files no vanilla file is named after (added stream files) are merged, whatever their
  format, once that format is supported; YMAPs are the only one now.
- A file name that resources ship for their own files rather than as one shared file, such as
  `_manifest.ymf`, never identifies a stream file across resources: it is neither an added
  stream file nor a replacement of a vanilla file of that name, and each resource's is deployed
  as its own.
- What the overwrite cache records about added stream files moves from `added_ymaps.json` to
  `added_stream_files.json`, with its types renamed; its format version is bumped, so the cache
  is rebuilt and the earlier file removed.

## Steps

### 1. Added stream files

- [x] Rename the overwrite cache's added-YMAP record: `AddedYmaps` → `AddedStreamFiles`,
  `AddedYmap` → `AddedStreamFile`, `AddedYmapSource` → `AddedStreamFileSource`,
  `added_ymaps.json` → `added_stream_files.json`, and their uses in the merge.
- [x] Rename the source cache's generic parts: `MergeInputs::added_ymaps` →
  `added_stream_files`, `is_added_ymap` → `is_added_stream_file` (taking the formats in
  `ADDED_FORMATS`, YMAP only), `write_added_ymaps` → `write_added_stream_files`. `added_ymap_relationships.json` stays, as parent relationships
  are a YMAP concept.
- [x] Update the TypeScript registry, docs (reference.md's "Added YMAPs" becomes "Added Stream
  Files", with its YMAP rules under it), the web sample README and the tests.

### 2. Names that identify no stream file

- [ ] One source-cache rule for the file names resources ship as their own (`_manifest.ymf`),
  applied wherever stream files are matched across resources or with vanilla: added stream
  files, vanilla-named files and stream conflicts.
- [ ] Test it with a `_manifest.ymf` in two resources; document it in reference.md.

### 3. Stream-file wording in generic code

- [ ] `ymap_diff_path` → `stream_file_diff_path`, as diff documents are kept by stream path.
- [ ] The overwrite warnings: match resource overwrites against every source stream file and
  the latest vanilla files of every format, and report one of a format overwrites do not
  support yet as `unsupported_format` instead of `missing_source`; their messages name stream
  files.
- [ ] Resource overwrites of unsupported formats are reported when loaded, not silently skipped by
  the merge.

### 4. Check the overwrite cache

- [ ] `--check` checks the overwrite cache: a stale digest, sources `added_stream_files.json`
  names that are missing, and history files `source_updates/` names that are missing.

## Out of Scope

- A format-independent diff document envelope and per-format item identity, thinning and merge
  policy: planned with YBN diff documents and overwrites (see TODO.md).
- Merging added stream files of formats other than YMAP.
