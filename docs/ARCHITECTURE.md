# Architecture

## Overview

This tool merges FiveM mod map (`.ymap`) files while avoiding conflicts between multiple mods. The pipeline is:

1. **Extract** (`src/core/extract`) - pulls `.ymap` files out of MLO mod resource directories (`asset/source`) into `asset/extracted`, also writing `_extracted_ymaps.txt` (names of replaced vanilla ymaps) and `_notextracted_resources.txt`.
2. **ymap -> xml** (`src/core/xmlconvert::Ymap2Xml`) - converts binary `.ymap` files into `.ymap.xml` via CodeWalker.Core by default, or the native Rust serializer with `--native-ymap-to-xml`.
3. **Merge** (`src/core/merge`) - combines vanilla and mod `.ymap.xml` files, tracking entity-level diffs (`src/core/format`, `structdiff`). Ymaps with only one mod reference (no merge needed) are copied as-is (still binary) into `asset/merged.xml/clone` instead of being re-emitted as xml.
4. **xml -> ymap** (`src/core/xmlconvert::Xml2Ymap`) - converts merged `.ymap.xml` back into binary `.ymap` via CodeWalker.Core by default, or the native Rust META/RSC7 writer with `--native-xml-to-ymap`.
5. **Deploy** (optional, `--output-resource-dir`) - overwrites a FiveM resource directory's `stream/ymap/merged`, `stream/ymap/clone` and `omit.txt` with this run's output.

`cargo run` with no arguments (`Command::Pipeline` in [src/cli/command.rs](../src/cli/command.rs)) runs all steps in order after a confirmation prompt (step 5 only if `--output-resource-dir` is given). Each of steps 1-4 is also available as an individual flag-based subcommand (`--extract-ymap`, `--merge-ymap-xml`, etc.) for manual use.

## CodeWalker bridge (ymap <-> xml conversion)

[CodeWalker](https://github.com/dexyfex/CodeWalker) is the reference implementation for GTA5 `.ymap` binary <-> XML conversion. Its `CodeWalker.Core.dll` is a managed .NET (netstandard2.0) assembly.

Instead of shelling out to CodeWalker.exe or a subprocess, this project hosts the .NET runtime **in-process**:

- `bridge/CodeWalker.Bridge` is a small C# class library that references a locally supplied `CodeWalker.Core.dll` (via the `CODEWALKER_CORE_DLL` env var, defaulting to `asset/CodeWalker.Core.dll` / `CodeWalkerCoreDllPath` MSBuild property) and exposes `[UnmanagedCallersOnly]` static methods (`PreloadNames`, `YmapToXml`, `XmlToYmap`, `GetLastError`) that wrap `CodeWalker.GameFiles.YmapFile`/`MetaXml`/`XmlMeta`.
- `build.rs` builds this bridge via `dotnet publish` whenever `dotnet` is on `PATH` and a CodeWalker.Core.dll is found (otherwise it emits a warning and skips it).
- `src/core/codewalker` hosts the CoreCLR runtime from Rust using the [`netcorehost`](https://crates.io/crates/netcorehost) crate (wrapping `hostfxr`), loads `bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll`, and resolves the exported functions as raw function pointers. A single `CodeWalker` instance must be reused for a whole batch conversion and only ever called from one thread (CodeWalker's `JenkIndex`/`JenkHash` caches are process-global statics).
- `src/core/xmlconvert` walks a directory of `.ymap`/`.ymap.xml` files. The default backend remains CodeWalker.Core. `--native-ymap-to-xml` uses the Rust decoder/XML serializer; `--native-xml-to-ymap` uses the Rust XML/META/RSC7 writer and builds its schema catalog from the extracted binary YMAP directory. Each direction can be selected independently; both flags together avoid CoreCLR in the pipeline.
- `src/core/format/gamefile` decodes/encodes RSC7 pages, maps system/graphics virtual addresses, parses/writes META schemas/enums/data blocks, and serializes/deserializes YMAP META XML. Bidirectional differential tests matched canonical CodeWalker XML for 623 checked-in YMAPs. Eight fixtures were excluded because their source CodeWalker XML contains `<error>` nodes for missing schema information.

### Why in-process hosting instead of a subprocess

Using a single hosted CLR process for the whole batch conversion is what makes the JenkIndex preload pass effective: cross-references between files only resolve to names if all relevant files were loaded into the *same* process before conversion. A subprocess-per-file design would lose this state between calls.

### Licensing note

`CodeWalker.Core.dll` is required for the default backend and XML-to-binary step. It must be supplied locally (`CODEWALKER_CORE_DLL` env var) and is gitignored (`bridge/**/bin`, `bridge/**/obj`, `asset/**/*`). It is never committed to or redistributed with this repository.
