# Architecture

## Overview

This tool merges FiveM mod map (`.ymap`) files while avoiding conflicts between multiple mods. The pipeline is:

1. **Extract** (`src/core/extract`) - pulls `.ymap` files out of MLO mod resource directories (`asset/source`) into `asset/extracted`, also writing `_extracted_ymaps.txt` (names of replaced vanilla ymaps) and `_notextracted_resources.txt`.
2. **ymap -> xml** (`src/core/xmlconvert::Ymap2Xml`) - converts binary `.ymap` files into `.ymap.xml` via the native Rust serializer by default, or CodeWalker.Core with `--use-codewalker-dll`.
3. **Merge** (`src/core/merge`) - combines vanilla and mod `.ymap.xml` files, tracking entity-level diffs (`src/core/format`, `structdiff`). Ymaps with only one mod reference (no merge needed) are copied as-is (still binary) into `asset/merged.xml/clone` instead of being re-emitted as xml.
4. **xml -> ymap** (`src/core/xmlconvert::Xml2Ymap`) - converts merged `.ymap.xml` back into binary `.ymap` via the native Rust META/RSC7 writer by default, or CodeWalker.Core with `--use-codewalker-dll`.
5. **Deploy** (optional, `--output-resource-dir`) - overwrites a FiveM resource directory's `stream/ymap/merged`, `stream/ymap/clone` and `omit.txt` with this run's output.

`cargo run` with no arguments (`Command::Pipeline` in [src/cli/command.rs](../src/cli/command.rs)) runs all steps in order after a confirmation prompt (step 5 only if `--output-resource-dir` is given). Each of steps 1-4 is also available as an individual flag-based subcommand (`--extract-ymap`, `--merge-ymap-xml`, etc.) for manual use.

`--check-stream-conflicts --input <DIR> --output <FILE>` uses the same manifest-aware resource discovery as extraction, scans each resource's `stream/` (or a stream directory directly), groups files by case-insensitive basename, and writes conflicting relative paths as JSON.

## CodeWalker bridge (ymap <-> xml conversion)

[CodeWalker](https://github.com/dexyfex/CodeWalker) is the reference implementation for GTA5 `.ymap` binary <-> XML conversion. Its `CodeWalker.Core.dll` is a managed .NET (netstandard2.0) assembly.

Instead of shelling out to CodeWalker.exe or a subprocess, this project hosts the .NET runtime **in-process**:

- `bridge/CodeWalker.Bridge` is a small C# class library that references a locally supplied `CodeWalker.Core.dll` (via the `CODEWALKER_CORE_DLL` env var, defaulting to `asset/CodeWalker.Core.dll` / `CodeWalkerCoreDllPath` MSBuild property) and exposes `[UnmanagedCallersOnly]` static methods (`PreloadNames`, `YmapToXml`, `XmlToYmap`, `GetLastError`) that wrap `CodeWalker.GameFiles.YmapFile`/`MetaXml`/`XmlMeta`.
- `build.rs` builds this bridge via `dotnet publish` whenever `dotnet` is on `PATH` and a CodeWalker.Core.dll is found (otherwise it emits a warning and skips it).
- `src/core/codewalker` hosts the CoreCLR runtime from Rust using the [`netcorehost`](https://crates.io/crates/netcorehost) crate (wrapping `hostfxr`), loads `bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll`, and resolves the exported functions as raw function pointers. A single `CodeWalker` instance must be reused for a whole batch conversion and only ever called from one thread (CodeWalker's `JenkIndex`/`JenkHash` caches are process-global statics).
- `src/core/xmlconvert` walks a directory of `.ymap`/`.ymap.xml` files. Native Rust is the default for both conversion directions. `--use-codewalker-dll` selects CodeWalker.Core for both directions. The Native XML-to-YMAP writer builds its schema catalog from the extracted binary YMAP directory.
- `src/core/format/gamefile` decodes/encodes RSC7 pages, maps system/graphics virtual addresses, parses/writes META schemas/enums/data blocks, and provides shared XML-tree parsing. `--to-xml`/`--from-xml` support YMAP, RSC-META YTYP/YMT, YND NodeDictionary, and all declared YBN Bounds and polygon XML types. Plain XML YMT files are preserved; binary PSO/PSIN/RBF YMT variants remain unsupported. CodeWalker differential tests cover real YBN/YMT/YND/YTYP fixtures, and an ignored test scans active YBN resources. Some YBNs still exceed the stored-Quantum tolerance after Native rebuild; the corpus comparison currently reports those cases. Native YBN output also omits the GeometryBVH acceleration tree, so in-game collision performance/behavior is not yet verified. Bidirectional differential tests matched canonical CodeWalker XML for 623 checked-in YMAPs. Eight fixtures were excluded because their source CodeWalker XML contains `<error>` nodes for missing schema information.

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
