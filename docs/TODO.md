# TODO

## GTA V Vanilla Archive Cache

- [x] Add `--generate-gtav-cache -i <GAME_DIR> -o <CACHE_DIR>` with a Cargo command alias, using CodeWalker RPF decryption and recursive YMAP extraction.
- [x] Discover and read all installed root base RPFs, then update.rpf and ordered dlclist.xml DLC overlays, including nested platform DLCs and title-update DLC patches.
- [x] Store named native additions and serialized YmapDiff replacements in each version's ymap directory, with cache_info.json/version_info.json metadata.
- [x] Preserve actual diff-processing logs per version in create_cache.log, including retained logs for failed runs.
- [x] Test cumulative comparison inputs, unchanged skipping, diff JSON round-trips, version log isolation and root RPF discovery.
- [x] Validate /mnt/gtav with all root RPFs: 103 version directories, 10,647 native additions and 1,937 deserializable YmapDiff JSON reports with per-version logs.
- [x] Generate MLO diff caches with `--generate-diff-cache`, optional GTAV cache path, shared single/multiple resource discovery, per-file closest-stage inference and per-resource latest-stage baselines.
- [x] Save inference scores, chosen vanilla stages, UTC generation times, hashes/provenance and generation logs with serialized YmapDiff output.
- [x] Preserve legacy snapshot reading and recover missing old-cache snapshots from verified RPF provenance.
- [x] Validate brofx_mansion_06: 198 scanned files, two YMAP diff reports against 0029-mpapartment; test multiple resources, ties, cumulative baselines, metadata fallback, corruption failures and real RPF recovery.
- [x] Test real RSC7 fixtures for complete native snapshot states, supported original-plus-diff chain replay, and closest middle-stage selection with final diff round-trip.
- [x] Keep merge-oriented YmapDiff semantics separate from lossless vanilla history deltas, preserving MLO merge behavior.
- [x] Implement schema-3 vanilla state reconstruction from original YMAP plus exact JSON deltas, with removals, cleared flags, metadata, entity order and predecessor/result integrity checks.
- [x] Remove the diff-only reconstruction test's ignore and verify zero full-model difference against real DLC-derived state without snapshots or game access; split diff-cache loading and tests into modules.
- [x] Move all GTAV cache implementation under src/core/gtav_cache/ and split generation, archive resolution, metadata, logging, I/O, stage storage, publication and tests while preserving existing API paths.
- [x] Stop generating native/ replacement snapshots and native metadata references; verify original YMAP plus JSON-only history reconstruction after temporary binaries are deleted.
- [x] Move all diff-cache implementation under src/core/diff_cache/ and split orchestration, resource processing, comparison, history metadata, I/O and report types while preserving the existing command API.
- [x] Add read-only --list-vanilla-versions filename lookup with optional GTAV cache path, ordered added/modified JSON output and shared metadata loading; verify extension-independent YMAP/YBN lookup.
- [ ] Define vanillaVersion/build-number mapping and support independently captured historical builds.
- [x] Cache YBN additions and exact binary-span JSON replacements alongside YMAP, restore byte-identical targets with SHA-256 checks, and include YBNs in vanilla version lookup.
- [x] Regenerate /mnt/gtav cache: 103 stages, 12,024 YBN additions and 472 replacements; verify byte-identical bh1_08_0.ybn reconstruction, version listing and all 22,671 native additions / 2,409 mixed diff artifacts.
- [x] Keep YMAP MLO diff-cache generation working with YBN-inclusive caches; record vanilla-matched unsupported types explicitly.
- [ ] Extend extraction and cache objects to native types other than YMAP/YBN.
- [x] Organize indexed YBN models under format/ybn/model, XML import/export under format/ybn/xml, native codecs under binary, and preserve old gamefile API exports.
- [x] Add structured YbnDiff/PolygonDiff extraction with duplicate-aware additions/removals, resolved material/vertex values, hierarchy ownership, Bounds metadata, tolerance and JSON serialization; verify the real mansion collision model.
- [ ] Add semantic YBN MLO diff-cache generation and structured delta apply/merge integration.
- [ ] Support update2.rpf and content/setup mount rules for complete engine snapshots.

## Resource Conversion Tests

- [x] Add explicit XML -> YBN/YMAP -> XML -> binary stability tests comparing first/second rebuilt bytes and re-exported XML; verify four META YMAP XML fixtures.
- [x] Resolve repeated YBN rebuild instability using preserved VertexQuantum, quantized-vertex BVH input, deterministic BVH ordering and final-order material/edge rebuilding; all six YBN/YMAP stability tests pass with exact binary/XML equality.
- [x] Verify shared-edge selection for three triangles against CodeWalker.Core behavior and confirm CodeWalker accepts Native XML with the optional VertexQuantum extension.
- [x] Format multiline text elements consistently in Native YBN/YND/META/PSO XML; verify YBN vertex/colour arrays remain rebuildable and preserve inline/empty values.
- [x] Support embedded-schema PSO export and bounded template-based `.pso.xml` rebuilds.
- [ ] Add general PSO allocation/rebuilding for array growth, longer strings, changed structure types, and checksum updates.

- [x] Extract local YMAP/YBN/YMT/YND/YTYP fixtures into `asset/sample` with source manifests.
- [x] Generate individual export and rebuild tests under `src/core/format/gamefile/test`.
- [x] Compare Native and CodeWalker exports and rebuilds; report byte identity separately.
- [x] Isolate expensive rebuilds with per-fixture time limits and produce JSON summaries.
- [x] Merge colliding YBN Bounds against vanilla using world-coordinate polygon additions/removals for supported Geometry children with a 5 mm match tolerance, retain child-level fallback for unsupported bounds, omit source collisions, and rebuild GeometryBVH in Native.
- [x] Generate Native GeometryBVH acceleration trees with polygon reorder and triangle edge remapping.
- [ ] Resolve the remaining YBN export/rebuild differences.
- [ ] Support the YTYP META array cases rejected by the current Native adapters.
- [ ] Investigate reference YMAP schema errors and the LOD-light hash rebuild discrepancy.
- [ ] Investigate YMT cases that CodeWalker cannot export; cover additional binary families as supported.