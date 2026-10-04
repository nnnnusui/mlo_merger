
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
