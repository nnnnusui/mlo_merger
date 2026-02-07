
# MLO Merger

[![SUSHI-WARE LICENSE](https://img.shields.io/badge/license-SUSHI--WARE%F0%9F%8D%A3-blue.svg)](https://github.com/MakeNowJust/sushi-ware)

A tool for merging FiveM mod map `.ymap` files to avoid conflicts between multiple mods.

## Status

- ✅ Core merge functionality is working
- 🚧 Additional utility features are under development

## Usage

### Basic Workflow

1. **Extract** - Extract MLO data from `.ymap` files
2. **Convert to XML** - Use CodeWalker to convert `.ymap` files to `.ymap.xml` format
3. **Merge XML** - Run the merge command to combine vanilla and mod files
4. **Convert to YMAP** - Use CodeWalker to convert merged `.ymap.xml` files back to `.ymap` format

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
