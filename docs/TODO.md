# TODO

## Resource Conversion Tests

- [x] Support embedded-schema PSO export and bounded template-based `.pso.xml` rebuilds.
- [ ] Add general PSO allocation/rebuilding for array growth, longer strings, changed structure types, and checksum updates.

- [x] Extract local YMAP/YBN/YMT/YND/YTYP fixtures into `asset/sample` with source manifests.
- [x] Generate individual export and rebuild tests under `src/core/format/gamefile/test`.
- [x] Compare Native and CodeWalker exports and rebuilds; report byte identity separately.
- [x] Isolate expensive rebuilds with per-fixture time limits and produce JSON summaries.
- [x] Merge colliding YBN Bounds against vanilla using world-coordinate triangle additions/removals for pure-triangle geometry children, retain child-level fallback for other bounds, omit source collisions, and rebuild GeometryBVH in Native.
- [x] Generate Native GeometryBVH acceleration trees with polygon reorder and triangle edge remapping.
- [ ] Resolve the remaining YBN export/rebuild differences.
- [ ] Support the YTYP META array cases rejected by the current Native adapters.
- [ ] Investigate reference YMAP schema errors and the LOD-light hash rebuild discrepancy.
- [ ] Investigate YMT cases that CodeWalker cannot export; cover additional binary families as supported.