# TODO

## YMAP Metadata Merge

- [x] Generate vanilla YMAP XML with Native conversion (11,472 RSC7 resources plus four PSO resources).
- [x] Merge parent and physics dictionaries using hash-aware vanilla-relative deltas.
- [x] Merge instanced-data links, represented prop references, and grass instance/batch deltas.
- [x] Prevent absent XML error tags from discarding valid instanced data on reload.
- [x] Validate only the 31 affected YMAPs and their 68 mod references, including XML reload.
- [x] Identify the four rejected vanilla inputs as PSO/PSIN and verify Native export/template rebuild with CodeWalker.
- [ ] Verify merged grass output through binary rebuild and in-game rendering, including extent handling.

## Resource Conversion Tests

- [x] Support embedded-schema PSO export and bounded template-based `.pso.xml` rebuilds.
- [ ] Add general PSO allocation/rebuilding for array growth, longer strings, changed structure types, and checksum updates.

- [x] Extract local YMAP/YBN/YMT/YND/YTYP fixtures into `asset/sample` with source manifests.
- [x] Generate individual export and rebuild tests under `src/core/format/gamefile/test`.
- [x] Compare Native and CodeWalker exports and rebuilds; report byte identity separately.
- [x] Isolate expensive rebuilds with per-fixture time limits and produce JSON summaries.
- [ ] Resolve the remaining YBN export/rebuild differences and generate acceleration trees.
- [ ] Support the YTYP META array cases rejected by the current Native adapters.
- [ ] Investigate reference YMAP schema errors and the LOD-light hash rebuild discrepancy.
- [ ] Investigate YMT cases that CodeWalker cannot export; cover additional binary families as supported.