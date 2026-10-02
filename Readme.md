
# MLO Merger

[![SUSHI-WARE LICENSE](https://img.shields.io/badge/license-SUSHI--WARE%F0%9F%8D%A3-blue.svg)](https://github.com/MakeNowJust/sushi-ware)

A tool for merging FiveM mod map `.ymap` files to avoid conflicts between multiple mods.

## Status

- ✅ Core merge functionality is working
- 🚧 Additional utility features are under development

## Usage

### Automated Pipeline (recommended)

Running `cargo run` with no arguments executes the full workflow in one go: extract -> convert to xml -> merge -> convert back to ymap. All intermediate paths (`vanilla/ymap.xml`, `extracted`, `extracted.xml`, `merged.xml`, `merged`, `blacklist.toml`, `log`) are resolved relative to a workspace directory (`asset` by default). It prompts for confirmation before running.

```bash
# defaults: workspace=asset, source-dir=<workspace>/source
cargo run

# also deploy into a FiveM resource directory (overwrites stream/ymap/{merged,clone} and omit.txt)
cargo run -- --workspace asset --source-dir asset/source --output-resource-dir asset/merged_mlo
```

This requires CodeWalker.Core.dll (from [CodeWalker](https://github.com/dexyfex/CodeWalker), GPL-3.0) to be available locally - it is **not** distributed with this repository. By default it is looked up at `<workspace>/CodeWalker.Core.dll` (e.g. `asset/CodeWalker.Core.dll`); set `CODEWALKER_CORE_DLL` to override the location:

```bash
# .NET SDK 8 is required to build the bridge (see .devcontainer/devcontainer.json's dotnet feature,
# or install manually: https://dotnet.microsoft.com/download)
export CODEWALKER_CORE_DLL=/path/to/CodeWalker.Core.dll  # optional, defaults to asset/CodeWalker.Core.dll
cargo run
```

`cargo build`/`cargo run` will build `bridge/CodeWalker.Bridge` (a small .NET class library wrapping CodeWalker.Core's ymap<->xml conversion) automatically via `build.rs` whenever `CODEWALKER_CORE_DLL` is set and `dotnet` is on `PATH`. If either is missing, the build prints a warning and the pipeline command will fail at runtime with instructions - the other flag-based commands below are unaffected.

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
