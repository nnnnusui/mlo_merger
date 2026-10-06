# MLO Merger

[![SUSHI-WARE LICENSE](https://img.shields.io/badge/license-SUSHI--WARE%F0%9F%8D%A3-blue.svg)](https://github.com/MakeNowJust/sushi-ware)

Merges FiveM YMAP resources and conflicting YBN collision files against vanilla data.
See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the processing flow and
[docs/TODO.md](docs/TODO.md) for remaining work.

## Requirements

- Rust stable.

```bash
cargo build
```

## CLI

The command interface is being reorganized. The current handlers only parse the
arguments and print a mock invocation; they do not read or write game resources.

```bash
cargo run -- --generate-vanilla -i /mnt/gtav -o asset/vanilla
cargo run -- --generate-vanilla-cache --vanilla asset/vanilla -o asset/vanilla-cache
cargo run -- --generate-source-cache -i asset/source --vanilla-cache asset/vanilla-cache -o asset/source-cache
cargo run -- --merge --vanilla-cache asset/vanilla-cache --source-cache asset/source-cache -o asset/merged
cargo run -- --to-xml collision.ybn -o exported
cargo run -- --from-xml exported/collision.ybn.xml -o rebuilt
cargo run -- --get-diff vanilla.ybn mod.ybn -o diff
```

`-o` defaults to the current directory. Common options include `--vanilla`,
`--vanilla-cache`, `--source-cache`, `--gamebuild`, `-f` to force the selected
operation, `-y` to skip confirmation, and repeatable `--step-name <name>` to
select pipeline stages. `--gamebuild` accepts a build number or version name.
Run `cargo run -- --help` for the full interface. Convenience aliases are
defined in [.cargo/config.toml](.cargo/config.toml).

## Limitations

- Cache stages do not reproduce the complete game-engine mount rules or historical installations.
- MLO semantic differences are reports, not lossless history patches.
- Conversion support depends on the resource family and available schemas. PSO rebuilding cannot grow arrays or allocated strings; RBF YMT is unsupported.
- Matching XML or stable rebuilds does not establish in-game load safety. Validate generated resources in the target game environment.

## Tests

```bash
cargo test --lib
cargo test --doc
```

Some tests require local game assets or CodeWalker and are ignored by default.
Game assets and CodeWalker binaries are not redistributed.