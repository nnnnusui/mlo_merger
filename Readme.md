
# MLO Merger

[![SUSHI-WARE LICENSE](https://img.shields.io/badge/license-SUSHI--WARE%F0%9F%8D%A3-blue.svg)](https://github.com/MakeNowJust/sushi-ware)

A tool for merging FiveM mod map `.ymap` files to avoid conflicts between multiple mods.

## Status

- ✅ Core merge functionality is working
- 🚧 Additional utility features are under development

## Usage

### Automated Pipeline (recommended)

Running `cargo run` with no arguments executes the full workflow in one go: extract -> convert to xml -> merge YMAPs -> rebuild YMAPs -> merge colliding YBN bounds against vanilla. All intermediate paths (`vanilla/ymap.xml`, `vanilla/ybn`, `extracted`, `extracted.xml`, `merged.xml`, `merged`, `merged_ybn`, `blacklist.toml`, `log`) are resolved relative to a workspace directory (`asset` by default). Existing generated outputs are listed and replaced after confirmation; pass `-y` to skip the prompt.

```bash
# defaults: workspace=asset, source-dir=<workspace>/source
cargo run

# also deploy into a FiveM resource directory (overwrites merged YMAP/YBN streams and omit.txt)
cargo run -- --workspace asset --source-dir asset/source --output-resource-dir asset/merged_mlo

# skip confirmation before replacing generated outputs
cargo run -- -y --workspace asset --source-dir asset/source --output-resource-dir asset/merged_mlo

# opt in to CodeWalker.Core.dll for both conversion directions
cargo run -- --use-codewalker-dll

# scan nested FiveM stream folders for duplicate basenames
cargo run -- --check-stream-conflicts --input asset/source --output asset/stream-conflicts.json
```

YMAP and YBN conversion use the Native Rust backend by default. Native YBN rebuilds generate GeometryBVH acceleration trees for collision geometry. `--use-codewalker-dll` selects CodeWalker.Core.dll (from [CodeWalker](https://github.com/dexyfex/CodeWalker)) instead for YMAP conversion and YBN rebuilds. The DLL must be available locally and is **not** distributed with this repository. Native XML-to-YMAP conversion builds its schema catalog from extracted binary YMAPs. Verify the applicable license before using or redistributing CodeWalker files. By default the DLL is looked up at `<workspace>/CodeWalker.Core.dll` (e.g. `asset/CodeWalker.Core.dll`); set `CODEWALKER_CORE_DLL` to override the location:

```bash
# .NET SDK 8 is required to build the bridge (see .devcontainer/devcontainer.json's dotnet feature,
# or install manually: https://dotnet.microsoft.com/download)
export CODEWALKER_CORE_DLL=/path/to/CodeWalker.Core.dll  # optional, defaults to asset/CodeWalker.Core.dll
cargo run
```

When `--use-codewalker-dll` is selected, `cargo build`/`cargo run` builds `bridge/CodeWalker.Bridge` automatically via `build.rs` if `CODEWALKER_CORE_DLL` and `dotnet` are available. The default Native pipeline does not require the .NET SDK or DLL.

### GTA V Vanilla Archive Cache

With the CodeWalker bridge built as described above:

```bash
cargo run -- --generate-gtav-cache -i /mnt/gtav -o asset/gtav-cache
# Cargo alias defined in .cargo/config.toml
cargo generate-gtav-cache -i /mnt/gtav -o asset/gtav-cache
```

Both input and output directories are required (`--input`/`--output` are also accepted).

Reads all installed root `.rpf` files (`common.rpf`, `x64a.rpf`, etc.) in filename
order, then `update/update.rpf` and the DLC archives listed in
`common/data/dlclist.xml`, in XML order. Nested platform DLCs are resolved from
the root RPFs; modern DLCs are read from `update/x64/dlcpacks`. YMAP resources
are exported as standalone native files with their resource headers restored.
GTA V Legacy and a local `GTA5.exe` are required; the game directory is read-only.

The cache uses decoded native filenames rather than hash filenames:

```text
gtav-cache/
  cache_info.json
  0000-base/
    create_cache.log
    version_info.json
    ymap/ch1_01.ymap
  0001-update/
    create_cache.log
    version_info.json
    ymap/ch1_01.ymap.diff.json
    ymap/new_map.ymap
```

New filenames are saved unchanged as `.ymap`; content replacements are compared
against the preceding cumulative version and saved as exact vanilla-model
deltas in `.ymap.diff.json` (GTAV cache schema 3). Unchanged content is omitted. Version logs include
the existing diff-detection logs. Failed-run logs are retained under `failed-*`.
Stage IDs (`0000-base`, `0001-update`, then position-prefixed DLC names) are
**not historical game build numbers**. Vanilla deltas record a predecessor,
all modeled field changes, entity order, and before/after model hashes. The
original YMAP plus JSON deltas reconstructs each parsed model without the game
or replacement snapshots. This does not recreate byte-identical compressed files.
See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). New generation does not create
`native/` directories or snapshot references. Original/new YMAPs are retained
under `ymap/`; replacement binaries exist only in temporary comparison storage.
Older cache snapshot references remain readable. Existing schema-2 merge-diff caches must
be regenerated to enable exact JSON-only history reconstruction.

### Vanilla Version Lookup

```bash
cargo run -- --list-vanilla-versions example.ymap
# Cargo alias; --gtav-cache defaults to asset/gtav-cache
cargo list-vanilla-versions example.ybn --gtav-cache asset/gtav-cache
```

Returns a JSON array of stages that introduced (`added`) or replaced (`modified`)
the filename, in cache order, with artifact paths and archive provenance.
Matching is case-insensitive. Unchanged stages are omitted and an unknown name
returns `[]`. This is a read-only metadata query: it does not extract RPFs,
generate caches, or create/rotate application logs.
Lookup is extension-independent and accepts `.ymap`, `.ybn`, or other cached
filenames. GTAV cache generation currently extracts only YMAPs; other types
will appear once their metadata is available.

### MLO Diff Cache

```bash
cargo run -- --generate-diff-cache --gtav-cache asset/gtav-cache \
  -i 'asset/source/[patron]/brofx_mansion_06' -o asset/diff-cache
# --gtav-cache defaults to asset/gtav-cache
cargo generate-diff-cache -i asset/source -o asset/all-diff-cache
```

Input may be one resource or a resources root with nested bracket groups. The
shared manifest-aware explorer discovers resources, then `stream/` and `streams/`
files are matched case-insensitively by basename against vanilla cache entries.
YMAP is currently the supported comparison format; unmatched files are listed
in metadata instead of copied or compared.

For each matched file, distinct changed-stage vanilla states are ranked by
model field differences (excluding name and block metadata). Ties choose the
newer changed stage. The latest of these per-file best stages becomes the
resource's baseline; all its matched files are then compared against that
stage's cumulative vanilla state. Each resource is inferred independently.

The empty output directory receives `diff_cache_info.json`, `create_cache.log`,
and `<resource>/resource_info.json` plus `<resource>/ymap/<stream-relative-path>.diff.json`.
Metadata includes UTC generation time, chosen version, all candidate scores,
hashes/provenance and unmatched paths. Failures are recorded as incomplete.
The example processed 198 stream files and generated two YmapDiff reports
against `0029-mpapartment`.

Comparison uses exact parsed states: schema-3 vanilla deltas are composed from
the original YMAP with full-model hash checks, while MLO merge diffs are never
used as exact history patches. Older caches can use retained
native objects or recover replacements from the original installed game and
CodeWalker, verifying SHA-256. A missing root `cache_info.json` can be recovered
from per-version metadata; stale schema-1 manifests provide game-directory
information only when newer per-version metadata exists.

### Basic Workflow (manual / individual steps)

1. **Extract** - Extract MLO data from `.ymap` files
2. **Convert to XML** - Use CodeWalker to convert `.ymap` files to `.ymap.xml` format
3. **Merge XML** - Run the merge command to combine vanilla and mod files
4. **Convert to YMAP** - Use CodeWalker to convert merged `.ymap.xml` files back to `.ymap` format

Steps 2 and 4 can also be run through a real CodeWalker.exe install (Windows) instead of this tool's bridge, if preferred.

### Commands

For detailed command information, please refer to:
- `--help` flag for each command
- `.cargo/config.toml` for command aliases
- `src/cli/command.rs` for implementation details

Convert a single native resource to XML in the same directory, or provide `-o` to choose an output directory:

```bash
cargo run -- --to-xml -i asset/merged_ybn/hi@sc1_18_0.ybn
```

`--check-stream-conflicts` uses the same manifest-aware resource discovery as extraction, then scans each resource's `stream/` recursively (or scans a `stream/` directory directly). It groups files by case-insensitive basename and writes duplicate names with their relative paths to the requested JSON file.

### Example: Extract

```bash
cargo run -- --extract-ymap -i asset/mlo/source -o asset/mlo/ymap.extracted --vanilla-dir asset/vanilla/ymap.xml
```

### Example: Merge YMAP XML Files

Merge multiple mod YMAP XML files with vanilla files:

```bash
cargo run -- --merge-ymap-xml \
  --vanilla-dir asset/vanilla/ymap.xml \
  --mod-dir asset/merged/ymap.xml \
  --mod-ymap-dir asset/mlo/ymap.extracted \
  --output-dir asset/merged/ymap.xml
```

#### Blacklist Configuration

You can filter out specific occlude models from being added during merge by using a blacklist configuration file:

```bash
cargo run -- --merge-ymap-xml \
  --vanilla-dir asset/vanilla/ymap.xml \
  --mod-dir asset/merged/ymap.xml \
  --mod-ymap-dir asset/mlo/ymap.extracted \
  --output-dir asset/merged/ymap.xml \
  --blacklist-config asset/blacklist.toml
```

See `asset/blacklist.toml` for configuration format.
