# TODO

## GTA V Vanilla Archive Cache

- [x] Add `--generate-gtav-cache -i <GAME_DIR> -o <CACHE_DIR>` with a Cargo command alias, using CodeWalker RPF decryption and recursive YMAP extraction.
- [x] Discover and read all installed root base RPFs, then update.rpf and ordered dlclist.xml DLC overlays, including nested platform DLCs and title-update DLC patches.
- [x] Store named native additions and serialized YmapDiff replacements in each version's ymap directory, with cache_info.json/version_info.json metadata.
- [x] Preserve actual diff-processing logs per version in create_cache.log, including retained logs for failed runs.
- [x] Test cumulative comparison inputs, unchanged skipping, diff JSON round-trips, version log isolation and root RPF discovery.
- [x] Validate /mnt/gtav with all root RPFs: 103 version directories, 10,647 native additions and 1,937 deserializable YmapDiff JSON reports with per-version logs.
- [ ] Compare a selected MLO against a selected vanilla stage.
- [ ] Define vanillaVersion/build-number mapping and support independently captured historical builds.
- [ ] Extend extraction and cache objects to native types other than YMAP.
- [ ] Support update2.rpf and content/setup mount rules for complete engine snapshots.

## Resource Conversion Tests

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