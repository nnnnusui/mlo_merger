# Architecture

## Overview

This tool merges FiveM mod map (`.ymap`) files while avoiding conflicts between multiple mods. The pipeline is:

1. **Extract** (`src/core/extract`) - pulls `.ymap` files out of MLO mod resource directories (`asset/source`) into `asset/extracted`, also writing `_extracted_ymaps.txt` (names of replaced vanilla ymaps) and `_notextracted_resources.txt`.
2. **ymap -> xml** (`src/core/xmlconvert::Ymap2Xml`) - converts binary `.ymap` files into `.ymap.xml` via the native Rust serializer by default, or CodeWalker.Core with `--use-codewalker-dll`.
3. **Merge** (`src/core/merge`) - combines vanilla and mod `.ymap.xml` files, tracking entity-level diffs (`src/core/format`, `structdiff`). Single-mod YMAPs are copied as binary clones only when their final parent references and hierarchy fields remain unchanged; otherwise they are rebuilt from patched original XML.
4. **xml -> ymap** (`src/core/xmlconvert::Xml2Ymap`) - converts merged `.ymap.xml` back into binary `.ymap` via the native Rust META/RSC7 writer by default, or CodeWalker.Core with `--use-codewalker-dll`.
5. **Deploy** (optional, `--output-resource-dir`) - overwrites a FiveM resource directory's `stream/ymap/merged`, `stream/ymap/clone` and `omit.txt` with this run's output.

`cargo run` with no arguments (`Command::Pipeline` in [src/cli/command.rs](../src/cli/command.rs)) runs all steps in order after a confirmation prompt (step 5 only if `--output-resource-dir` is given). Each of steps 1-4 is also available as an individual flag-based subcommand (`--extract-ymap`, `--merge-ymap-xml`, etc.) for manual use.

`--check-stream-conflicts --input <DIR> --output <FILE>` uses the same manifest-aware resource discovery as extraction, scans each resource's `stream/` (or a stream directory directly), groups files by case-insensitive basename, and writes conflicting relative paths as JSON.

## YMAP Metadata Merging

The XML pipeline remains in place. `YmapDiff` also merges `parent`,
`physics_dictionaries`, and `instanced_data` instead of skipping those changes.
Parent, dictionary, ImapLink, prop-reference, and grass-archetype comparisons
resolve names and `hash_XXXXXXXX` to the same Jenkins hash.

- A changed parent or ImapLink replaces vanilla; conflicting changes keep the
	first mod's value in the existing processing order and log a warning.
- Dictionary and represented prop-reference lists apply vanilla-relative
	additions/removals. Additions are deduplicated by hash; an unchanged mod does
	not restore another mod's removal.
- Grass batches are identified by archetype, AABB, scale, LOD settings, and
	terrain orientation. Within a matching batch, packed positions identify
	instances; changes to their payloads are merged independently. Instance or
	batch removal wins over a conflicting modification; conflicting payload
	edits at one position keep the first change.
- Changed batch metadata is treated as removal of the old batch plus addition
	of the new batch. Packed coordinates are never mixed across different AABBs.
- Valid instanced-data XML omits the optional `error` element. Emitting an
	empty `<error/>` would make the reader discard the grass data on reload.

Regenerate vanilla XML with Native conversion:

```sh
cargo run -- --to-xml --input asset/vanilla/ymap --output asset/vanilla/ymap.xml
```

The initial run produced 11,472 RSC7 XML files. Four inputs
(`cs1_railwyc.ymap`, `cs1_railwyc_long_0.ymap`, `id2_17.ymap`,
`id2_17_strm_0.ymap`) are valid PSO/PSIN rather than RSC7 resources.
Native export now produces their `.ymap.pso.xml` files as well; none belonged
to the targeted merge set.

### Stable Parent References

`ymap_parent_refs` resolves each original `parentIndex` before entity diffing.
An eligible local LOD parent is used unless flag bit 3 declares an external
parent. Otherwise resolution uses the original parent YMAP from the same mod
resource, falling back to vanilla. Entity arrays follow CodeWalker's runtime
order: `CEntityDef` entries first, then `CMloInstanceDef` entries.

The merge uses map-scoped internal entity identities and temporary negative
parent handles, not positional indices. Unique nonzero GUIDs identify entities;
zero or duplicate GUIDs use GUID, entity type, archetype, position, rotation,
and scale as a conservative fallback. Exact indistinguishable duplicates are
not guessed: a GUID-keyed merge that would discard them is rejected, while
unchanged clones retain their full original array. Original GUID values are
restored before output, including zero and duplicate values.

After all merges, handles are resolved against final runtime entity arrays.
Local LOD ordering and the declared external parent map are checked. Removed
parents detach children (`parentIndex = -1`, bit 3 cleared); HD children become
ORPHANHD. Unresolved indices are retained only when the original parent layout
is unchanged, or when the parent is unavailable in both inputs and outputs.
Other unresolved layout changes fail rather than silently linking another
entity. Parents with changed layouts have `numChildren` recalculated using
resolved links from planned maps, including dependent vanilla children.

Changed parent layouts trigger discovery of their vanilla child YMAPs, even
when those children have no mod override. Clone/vanilla children whose indices,
flags, LOD levels, or child counts change are promoted to rebuilds. Their
original XML is patched only for these fields, preserving entity order,
extensions, and other opaque payloads. The existing modeled-field limitation
still applies to ordinary multi-mod XML merges.

`_copy_targets.txt` contains only unchanged binary clones.
`_managed_ymaps.txt` scopes cleanup of obsolete generated binaries during both
pipeline backends and Native `--from-xml`; unrelated files are not deleted.
Promotion/demotion also removes obsolete same-name XML or clone output.

The ignored `merge_relinks_cloned_road_child` regression reproduces the
`hei_kt1_rd_strm_1` road GUID `3742112198`: its source parent index 198 resolves
to GUID `4241491920`, which is index 200 in the merged parent. It also checks
clone promotion, original entity counts, and stale clone removal.
`relinked_road_parent_survives_native_and_codewalker_rebuild` verifies the
Native/DLL binary rebuilds retain that link, their exports match, and entity
GUID order is preserved. Run the workflow regression before the binary test:

```sh
cargo test --lib merge_relinks_cloned_road_child -- --ignored --nocapture
CODEWALKER_CORE_DLL=/workspace/asset/CodeWalker.Core.dll cargo test --test codewalker_roundtrip relinked_road_parent_survives_native_and_codewalker_rebuild -- --ignored --nocapture
```

Local full-corpus validation covers 550 modded YMAP names, promotes 27
clone/vanilla children, and rebuilds 95 XML files natively without conversion
failures. Separate output is under `asset/merge_validation/lod_parent_all`
(`merged.xml/clone` for unchanged binaries, `merged` for rebuilt binaries).
No deployed resources are overwritten. In-game verification remains required.

### Native YMAP Runtime Verification

XML parity alone does not establish game-load safety. The user observed
in-game crashes with Native rebuilt `h4_mph4_terrain_02_grass_0.ymap` and
`bkr_id1_09.ymap`, while CodeWalker GUI imports of the same merged XML loaded
correctly. The ignored `rebuild_crashing_merged_ymaps_with_codewalker` test
generates only these two DLL binaries under `asset/merged_codewalker_dll`
without overwriting Native or GUI outputs. Both DLL binaries were byte-identical
to the known-good GUI versions; all three variants re-export equivalent XML.
Comparison identified three Native writing defects invisible to XML parity:

- `MetaStructureInfo.StructureKey`, `MetaEnumInfo.EnumKey`, and additional
	schema metadata were discarded during parsing. The writer substituted name
	hashes for layout keys and zeroed attributes. These fields are now preserved.
- `FilePagesInfo.SystemPagesCount` used weighted page-size units rather than
	the actual number of pages. The small crash case declared 18 pages for two
	actual pages; the grass case declared 239 for 114 actual pages.
- Native contiguous allocation could split META data blocks across resource
	pages (one block in the small case, 107 in the grass case). CodeWalker packs
	each block inside a page. Native `from_pages` now allocates one sufficiently
	large power-of-two page per nonempty region, keeping contiguous blocks intact.
	This trades additional padding for a straightforward valid runtime layout.

The `native_crash_rebuilds_preserve_runtime_schemas` ignored test compares every
used schema/enum with the known-good DLL, checks the page count and block
containment, and compares re-exported XML. Both cases pass. Corrected Native
files are saved in `asset/merged_native_fixed`; runtime re-testing by the user
is still required. Existing Native/GUI/DLL files and deployed resources are
not overwritten. Profiles and XML are under `asset/merge_validation/crash_rebuild`
and `asset/merge_validation/native_crash_fix`.

```sh
CODEWALKER_CORE_DLL=/workspace/asset/CodeWalker.Core.dll cargo test --test codewalker_roundtrip rebuild_crashing_merged_ymaps_with_codewalker -- --ignored --nocapture
```

### Distant LOD Light Rebuilds

The light XML serializer formerly emitted vector elements named
`XmlPositionChildValueAttr` rather than `Item`. The Native META builder only
counted `Item`, silently wrote an empty position descriptor, and retained a
nonempty RGBI array. Six merged `vw_distlodlights_medium` resources (010, 012,
015, 019, 022, 028) were affected. The builder now accepts the legacy vector
tag for `FloatXYZ`, and the serializer emits standard `Item` elements.
LOD/distant-light serializers omit absent `error` tags; an empty legacy
`<error/>` no longer discards otherwise valid lights during XML/model reload.
Unrecognized structure-array elements and mismatched distant-light
position/RGBI counts are rejected instead of creating malformed binaries.

`native_merged_distant_lights_match_codewalker` rebuilds the affected merged
XML, verifies that position blocks exist, and compares DLL re-exports. All six
cases passed. Corrected files are under `asset/merged_native_fixed`; DLL
reference files, XML, and `results.json` are under
`asset/merge_validation/distant_lights_fix`. In-game re-testing is still needed.

### PSO Conversion and Name Resolution

`gamefile/pso.rs` parses big-endian PSIN, PMAP, PSCH, and string sections.
PSO pointers use CodeWalker's per-u32 endian convention, not a full u64 byte
reversal. Embedded schemas drive XML export of supported structures, pointer
arrays, scalar/vector arrays, strings, enums, and flags. Unsupported types
fail explicitly; support is not claimed for every PSO family.

Native `--to-xml` identifies PSO by its PSIN header and writes `.pso.xml`.
`--from-xml` uses a matching original PSO under `--schema-dir` as a template
(a single original file may also be supplied). Existing numeric/reference
fields and strings within their existing capacity can be updated. Array
lengths and structure types cannot change; editing a checksummed PSO is
rejected. Unknown bytes and sections are preserved. This is template editing,
not general PSO creation. Four real YMAPs passed Native export and rebuild
comparison against the hosted DLL; the bridge explicitly routes PSIN YMAPs
through `PsoFile` rather than the RSC7-only byte loader.

CodeWalker resolves names through built-in `MetaNames` plus its process-global
`JenkIndex`. PSO STRF/STRS strings and RSC META strings populate that index;
the GUI additionally indexes GTA RPF filenames and derived names via
`RpfManager.BuildBaseJenkIndex`. The bridge does not load GTA archives itself.
Native exports use built-in schema/enum names, embedded strings, and caller
provided shared names. An unknown model hash remains `hash_XXXXXXXX`; it
cannot be reversed without a corresponding name source.

The ignored workflow test selects only YMAP names with the three unsupported
warnings in `asset/log/mlo_merger_20261002_154241.log`, stages all their mod
references, and executes `MergeYmapXml::run` with `rebuild_all` enabled:

```sh
cargo test --lib core::merge::run::tests::merge_logged_unsupported_ymaps -- --ignored --nocapture
```

It verifies parents, dictionary deltas, and grass payloads after XML reload,
and checks that the targeted log contains no unsupported-change warnings.
The local run passed for 31 YMAPs and 68 mod references. Outputs, `merge.log`,
and `results.json` live under `asset/merge_validation/unsupported_fields`.
The grass map retained 262 batches and 110,478 instances. This is XML-level
merge validation, not a full deployment or in-game rendering test; existing
extent-handling rules remain unchanged.

## CodeWalker bridge (ymap <-> xml conversion)

[CodeWalker](https://github.com/dexyfex/CodeWalker) is the reference implementation for GTA5 `.ymap` binary <-> XML conversion. Its `CodeWalker.Core.dll` is a managed .NET (netstandard2.0) assembly.

Instead of shelling out to CodeWalker.exe or a subprocess, this project hosts the .NET runtime **in-process**:

- `bridge/CodeWalker.Bridge` is a small C# class library that references a locally supplied `CodeWalker.Core.dll` (via the `CODEWALKER_CORE_DLL` env var, defaulting to `asset/CodeWalker.Core.dll` / `CodeWalkerCoreDllPath` MSBuild property) and exposes `[UnmanagedCallersOnly]` static methods (`PreloadNames`, `YmapToXml`, `XmlToYmap`, `GetLastError`) that wrap `CodeWalker.GameFiles.YmapFile`/`MetaXml`/`XmlMeta`.
- `build.rs` builds this bridge via `dotnet publish` whenever `dotnet` is on `PATH` and a CodeWalker.Core.dll is found (otherwise it emits a warning and skips it).
- `src/core/codewalker` hosts the CoreCLR runtime from Rust using the [`netcorehost`](https://crates.io/crates/netcorehost) crate (wrapping `hostfxr`), loads `bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll`, and resolves the exported functions as raw function pointers. A single `CodeWalker` instance must be reused for a whole batch conversion and only ever called from one thread (CodeWalker's `JenkIndex`/`JenkHash` caches are process-global statics).
- `src/core/xmlconvert` walks a directory of `.ymap`/`.ymap.xml` files. Native Rust is the default for both conversion directions. `--use-codewalker-dll` selects CodeWalker.Core for both directions. The Native XML-to-YMAP writer builds its schema catalog from the extracted binary YMAP directory.
- `src/core/format/gamefile` decodes/encodes RSC7 pages, maps system/graphics virtual addresses, parses/writes META schemas/enums/data blocks, and provides shared XML-tree parsing. `--to-xml`/`--from-xml` support YMAP, RSC-META YTYP/YMT, YND NodeDictionary, and all declared YBN Bounds and polygon XML types. Plain XML YMT files are preserved. PSIN uses the embedded-schema PSO adapter for its supported types and template-based rebuilds; RBF YMT remains unsupported. CodeWalker differential tests cover real YBN/YMT/YND/YTYP fixtures and four PSO YMAPs, and an ignored test scans active YBN resources. Some YBNs still exceed the stored-Quantum tolerance after Native rebuild; the corpus comparison currently reports those cases. Native YBN output also omits the GeometryBVH acceleration tree, so in-game collision performance/behavior is not yet verified. Bidirectional differential tests matched canonical CodeWalker XML for 623 checked-in YMAPs. Eight fixtures were excluded because their source CodeWalker XML contains `<error>` nodes for missing schema information.

### Real-resource regression tests

The `gamefile_samples` integration target lives in `src/core/format/gamefile/test`.
Each format has a separate module and generated case list, with individual
`to_xml` and `from_xml` tests for each actual source fixture. The tests call the
same native adapters used by `--to-xml` and `--from-xml`. DLL/asset-dependent
tests are ignored by default; comparator unit tests run normally.

Extract local fixtures and regenerate the case lists:

```sh
cargo test --test gamefile_samples extract_source_samples -- --ignored --nocapture
```

Fixtures use `asset/sample/{fileType}/resourceName___filename`, with a JSON
manifest recording the original source path. Discovery excludes `_backup` and
`_omit`. Identical duplicates within a resource are deduplicated; differing
same-name variants include their relative subdirectory in the resource label
to avoid overwriting data. Assets remain gitignored and are not redistributed.

Run each direction and then summarize the per-file JSON reports:

```sh
CODEWALKER_CORE_DLL=/workspace/asset/CodeWalker.Core.dll cargo test --test gamefile_samples ::to_xml -- --ignored
CODEWALKER_CORE_DLL=/workspace/asset/CodeWalker.Core.dll cargo test --test gamefile_samples ::from_xml -- --ignored --test-threads=8
cargo test --test gamefile_samples summarize_sample_results -- --ignored --nocapture
```

Export comparisons share one CLR worker thread. Rebuild comparisons use one
isolated test process per fixture, with a 60-second limit configurable via
`SAMPLE_TIMEOUT_SECONDS`; this prevents a costly DLL save from blocking the
entire corpus. Each process hosts the DLL in-process on a single thread.
`CODEWALKER_BRIDGE_DLL` can override the published bridge path.

`asset/sample/reports/{fileType}` holds per-direction results;
`asset/sample/reports/summary.json` aggregates failures without silently
skipping unsupported resources or CodeWalker errors. Comparisons normalize
XML whitespace, attribute order, numeric spelling (including hexadecimal
integers), and resolved hash names. Direct YBN export compares vertex f32 bits;
YBN rebuild compares stored-Quantum tolerances and polygon material contents,
allowing BVH polygon reorder. This checks semantic XML parity, not identical
XML formatting or in-game collision behavior.

`from_xml` feeds the same CodeWalker source XML to both writers and compares
their DLL re-exports. Reports separately record source XML preservation and
exact byte identity. Byte identity is not required: XML does not retain RSC
page layout, padding, compression choices, all unknown fields, or derived BVH
data. Even CodeWalker's own rebuild is often not byte-identical.

The initial local corpus contains 2,462 fixtures: 1,296 YMAP, 582 YBN, 233 YMT,
41 YND, and 310 YTYP. Export comparison passed 2,127 and failed 335; rebuild
comparison passed 1,866 and failed 596. Known failures include reference YMAP
schema errors, reference YMT conversion errors, unsupported Native YTYP META
arrays, and YBN rebuild differences. These ignored tests intentionally remain
red for those cases rather than claiming full format coverage.

### Why in-process hosting instead of a subprocess

Using a single hosted CLR process for the whole batch conversion is what makes the JenkIndex preload pass effective: cross-references between files only resolve to names if all relevant files were loaded into the *same* process before conversion. A subprocess-per-file design would lose this state between calls.

### Licensing note

`CodeWalker.Core.dll` is only needed when `--use-codewalker-dll` is specified. It must be supplied locally (`CODEWALKER_CORE_DLL` env var) and is gitignored (`bridge/**/bin`, `bridge/**/obj`, `asset/**/*`). It is never committed to or redistributed with this repository.
