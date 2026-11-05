# MLO Merger

[![SUSHI-WARE LICENSE](https://img.shields.io/badge/license-SUSHI--WARE%F0%9F%8D%A3-blue.svg)](https://github.com/MakeNowJust/sushi-ware)

Merges FiveM YMAP resources and conflicting YBN collision files against vanilla data.

| Document | Contents |
|---|---|
| [Command reference](docs/reference.md) | Every operation, its files and the merge behavior |
| [FiveM integration](docs/usage.md) | Calling the executable from a resource, its JSON outputs and overwrite documents |
| [Architecture](docs/ARCHITECTURE.md) | Workflow and design decisions |
| [Development](docs/development.md) | Building, testing, type generation and third-party notices |
| [Roadmap](docs/TODO.md) | Remaining work |

## Generate Vanilla Data With The GUI

Starting the executable without arguments, for example by double-clicking it, opens a
window that generates the vanilla archive:

1. Choose the GTA V Legacy directory. Common Steam, Epic and Rockstar installs are
   detected.
2. Choose `magic.dat`, or press **Download from GitHub** to save CodeWalker's copy next to
   the executable. The link next to it opens the file's GitHub page.
3. Press **Generate vanilla**. `vanilla/` is created next to the executable; upload it to
   the server as `asset/vanilla`.

The window is in English; switch it to Japanese with the language selector. Its log is
also written to `log/` next to the executable. Any argument, such as `--pipeline`, runs
the command line instead.

## Run The Pipeline

```bash
mlo_merger --pipeline
mlo_merger --pipeline -i resources -o merged_mlo --gamebuild 3095
```

The pipeline extracts vanilla when its archive is missing, updates the vanilla and source
caches, merges, then deploys, reusing every unchanged stage. Resources are read from
`asset/source` and the deployable resource is written to `asset/merged_mlo`; see the
[command reference](docs/reference.md#pipeline) for every path and option.

**Source-cache generation moves vanilla-named YMAP/YBN files out of resource stream
directories** into `asset/source-cache`; `--restore-source-cache` moves them back.

## Supported Files

Each stream file type is supported at one of three levels:

| Level | What happens to resources replacing a vanilla file |
|---|---|
| **Diff** | Each source's differences from vanilla are written as a diff document; every source is merged from its diff, and [overwrites](docs/reference.md#overwrites) can leave chosen differences out |
| **Merge** | Sources replacing the same vanilla file are combined from the files themselves; there are no diff documents or overwrites |
| **Unsupported** | Files stay in their resources and are not merged, so the game loads one of the same-named files. Name collisions between resources are only reported in `source-cache/stream_conflicts.json` |

| Extension | Contents | Level | Other operations |
|---|---|---|---|
| `.ymap` | Map placements: entities, MLO instances, car generators, grass, occluders, LOD lights | Diff | `--read`, `--find`, XML conversion |
| `.ybn` | Static collision bounds | Merge | `--find ybn-position` and `file-from-position`, XML conversion |
| `.ytyp` | Archetype definitions, including MLO interiors | Unsupported | XML conversion |
| `.ymt` | Metadata such as scenario points | Unsupported | XML conversion |
| `.ynd` | Vehicle and ped path nodes | Unsupported | XML conversion |
| `.ynv` | Navigation meshes | Unsupported | |
| `.ymf` | Manifests such as `_manifest.ymf` (map dependencies) | Unsupported | |
| `.ydr` | Drawables (models) | Unsupported | |
| `.ydd` | Drawable dictionaries | Unsupported | |
| `.yft` | Fragments (breakable or physical models) | Unsupported | |
| `.ytd` | Texture dictionaries | Unsupported | |
| `.ywr` | Water reflection data | Unsupported | |
| `.ycd` | Clip dictionaries (animations, including animated props) | Unsupported | |
| Others | | Unsupported | |

## Commands

| Operation | Command |
|---|---|
| [Run every stage](docs/reference.md#pipeline) | `--pipeline` |
| [Extract vanilla from the game](docs/reference.md#individual-operations) | `--generate-vanilla` |
| [Build the vanilla cache](docs/reference.md#game-builds) | `--generate-vanilla-cache` |
| [Build the source cache](docs/reference.md#source-cache) | `--generate-source-cache` |
| [Move cached files back](docs/reference.md#restoring-resources) | `--restore-source-cache` |
| [Merge](docs/reference.md#merge) | `--merge` |
| [Deploy](docs/reference.md#deploy) | `--deploy` |
| [Search merged data](docs/reference.md#inspect-existing-data) | `--find` |
| [Write YMAP models as JSON](docs/reference.md#read-stream-file-models) | `--read` |
| [Convert to and from XML](docs/reference.md#convert-files) | `--to-xml`, `--from-xml` |
| [Check the stores for inconsistencies](docs/reference.md#check-the-stores) | `--check` |

Run `mlo_merger --help` for every option.

Release archives include the third-party license notices (`THIRD_PARTY_NOTICES.md`) and
TypeScript types for the JSON files (`asset/gen_mlo_merger.ts`).
