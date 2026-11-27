# Web Demo Sample

The workspace the page in `web/` seeds before running the merger. Everything is original: names
are invented (`smpl_*`, the DLCs `smpl_winter`, `smpl_patch_01` and `smpl_summer`, and the game
builds 1, 100 and 200), and everything sits around (-7100, 7100), in the sea off the map, so no
game data is shipped. `src/app/map/web_sample` builds every file here; regenerate them with

```bash
MLO_WRITE_SAMPLES=1 cargo test --lib web_sample
```

and its tests check that the files are current and each example's outcome, one test each.
`files.json` lists the files the page seeds, all at once.

Each resource shows one behaviour of the merger a user meets on real data and is named after
it. The examples below give the files each adds or changes and what to look at after a pipeline
run (`--pipeline`, then `--pipeline --gamebuild 100` where the example says so). Paths are below
`asset/`: resources under `map/source/`, the stores the run writes under `map/` (`vanilla-cache`,
`source-cache`, `merged`, `merged_mlo`).

## Generations

DLCs ship vanilla YMAPs again under a prefix of their own, and the game loads the one the packs'
change sets leave in place; merges deploy only that one and carry changes to it.

| # | Example | Files | What to look at |
|---|---|---|---|
| 1 | The block's edits carried to its DLC generations, and the square, which `smpl_winter` replaces, omitted for its successor | `smpl_street_tweaks`, `smpl_block_rework`, `smpl_left_behind`; vanilla `winter_smpl_block_01.ymap`, `summer_smpl_block_01.ymap`, `winter_smpl_square.ymap` | `merged/generations/{resource}/0000-base/*.diff.json`; `vanilla-cache/ymap_generations.json` `successors`; `merged/_omit.txt`; `superseded_by` in `merged/cache_info.json` and `merged/report/index.json` |
| 2 | A YMAP first shipped after build 100 | `smpl_summer_only/stream/summer_smpl_block_01.ymap` | With `--gamebuild 100`: `newer_than_build` in `vanilla-cache/generations.json`, `newer_files` in `vanilla-cache/cache_info.json`, the resource's diff taken against `0004-smpl_summer`, the flag's turn in `merged/ymap/winter_smpl_block_01.ymap` |
| 3 | Changes left behind, one of each reason: the bench's removal (`already_absent`), the bin's turn (`no_loaded_counterpart`) and a sign added below the statue's LOD (`addition_not_carried`) | `smpl_left_behind/stream/smpl_square.ymap` | `merged/generations/left_behind.json`; `left_behind` in the square's report |
| 4 | YBN candidates: `smpl_summer` splits the yard's collision into two YBNs; the post's removal goes to both, which hold it, and the crate to the western one, which covers it | `smpl_yard_split/stream/smpl_yard.ybn`; vanilla `summer_smpl_yard_0.ybn`, `summer_smpl_yard_1.ybn` | `candidates` in `vanilla-cache/ybn_generations.json`; `merged/generations/smpl_yard_split/0001-update/smpl_yard.ybn/`; the resource's YBN in `_omit.txt`, not deployed. With `--gamebuild 100` the yard is loaded and merged instead |

## Older Versions Of A Vanilla File (Source Baselines)

A resource made from an older vanilla version would revert every later change; the source cache
takes the version it differs least from as its baseline and keeps only what it changed there.

| # | Example | Files | What to look at |
|---|---|---|---|
| 5 | Made from the title update's park, moving the first oak further: the diff leaves out the bench `smpl_patch_01` added and the oak `smpl_summer` swapped for the kiosk | `smpl_old_park/stream/smpl_block_02.ymap` | `baseline` (`0001-update`) and `diffs` in `source-cache/diff/smpl_old_park/smpl_block_02.ymap.diff.json` |
| 6 | The resources' collisions of the block made before `smpl_summer` added the kiosk's, which the merged collision keeps | `smpl_street_tweaks`, `smpl_block_rework`: `smpl_block_01.ybn` | `baseline` (`0000-base`) of their YBN diffs; `merged/ybn/smpl_block_01.ybn` |
| 7 | A LOD light pair made before the title update's fourth light: both YMAPs keep it, and the light removed from one YMAP only is removed from both | `smpl_old_lights/stream/lodlights_small001.ymap`, `distlodlights_small001.ymap` | `baseline` of both diffs; the `implied` removal in the distant diff; `merged/ymap/*lodlights_small001.ymap` |
| 8 | Matching `smpl_winter`'s rink before the title update's patches: it changes nothing, and nothing is deployed, which would undo the patch | `smpl_vanilla_rink/stream/winter_smpl_rink.ymap` | `baseline` (`0002-smpl_winter`); no `merged/ymap/winter_smpl_rink.ymap`; the source in `_omit.txt` and `made_from_older` in `merged/cache_info.json`; nothing in `merged_mlo` |

## Game Builds And DLC Content

| # | Example | Files | What to look at |
|---|---|---|---|
| 9 | Per-build title-update patches: `smpl_winter`'s rink changes for build 100 and again for build 200, the installed game's | vanilla `0002-smpl_winter/update_rpf/100/`, `update_rpf/200/` | `title_updates` and `patches` in `vanilla/cache_info.json`; the rink's `object` in `vanilla-cache/cache_info.json` with and without `--gamebuild 100`; `--read --all --gamebuild 100` |
| 10 | DLC activity: `smpl_winter` invalidates the pier's archive with nothing in its place and replaces the square's; its installed patch holds a hut in an archive it does not declare | vanilla `smpl_pier.ymap`, `smpl_square.ymap`, `winter_smpl_hut.ymap` | `vanilla-cache/dlc_activity.json` (`invalidated`, `stands_for`, `not_declared`); `inactive` in `vanilla-cache/generations.json` |
| 11 | Builds 1, 100 and 200 ending at different stages | `config.toml` | `latest_version` in `vanilla-cache/cache_info.json`; the files the run's report adds and removes |

## Entity Parents

| # | Example | Files | What to look at |
|---|---|---|---|
| 12 | Parent indices repaired after the block's empty SLOD is removed | `smpl_block_rework/stream/maps/smpl_block_01_lod.ymap` | The houses' `parentIndex` in `merged/ymap/smpl_block_01.ymap` and its generations |
| 13 | Removing every house below the row's SLOD: their LODs lose all their children and are removed, and the SLOD in turn | `smpl_houses_gone/stream/smpl_row.ymap` | `merged/ymap/smpl_row_lod.ymap` |
| 14 | A parent GUID two LOD entities share: the crane's reference is kept by index | `smpl_shared_parent_guid/stream/smpl_quay.ymap` | The crane's `parentIndex` in `merged/ymap/smpl_quay.ymap` |
| 15 | A house whose LOD the resource removes: detached, `LODTYPES_DEPTH_ORPHANHD`, with a warning; the next house points one lower | `smpl_orphaned_house/stream/smpl_row_lod.ymap` | `merged/ymap/smpl_row.ymap`; the log |

## Combining Sources

| # | Example | Files | What to look at |
|---|---|---|---|
| 16 | One change not applied for each reason of the merge report: two resources moving the park's bench alike (`identical_duplicate`), turning the fountain each its own way (`earlier_source_wins`), one turning the oak another removes (`removal_wins`); one resource turning the square's birch in both generations (`newer_generation_wins`) and planting the same flower bed in both (`same_addition_newer`) | `smpl_bench_moved`, `smpl_bench_moved_too`, `smpl_park_tweaks`: `smpl_block_02.ymap`; `smpl_square_both`: `smpl_square.ymap`, `winter_smpl_square.ymap` | `merged/report/vanilla/0004-smpl_summer/ymap/smpl_block_02.ymap.json`, `merged/report/vanilla/0002-smpl_winter/ymap/winter_smpl_square.ymap.json` |
| 17 | An added map shipped by two versions of a resource, combined: the shared sign once, the crate once as placed again under another GUID, the label following it | `smpl_custom_props`, `smpl_custom_props_v2`: `smpl_custom_01.ymap`; `smpl_custom_props/stream/smpl_custom_02.ymap` | `merged/ymap/smpl_custom_01.ymap`, `smpl_custom_02.ymap`; `overwrite-cache/added_stream_files.json` |
| 18 | Rounded GUIDs: a tool passed the park's GUIDs through a float; the entities pair with vanilla's, and only the kiosk's turn is merged | `smpl_rounded_guids/stream/smpl_block_02.ymap` | The resource's diff; the kiosk's GUID in `merged/ymap/smpl_block_02.ymap` |

## LOD Light Pairs

| # | Example | Files | What to look at |
|---|---|---|---|
| 19 | A light removed from both YMAPs of the pair, reaching the winter pair | `smpl_street_tweaks/stream/lodlights_small001.ymap`, `distlodlights_small001.ymap` | `merged/ymap/winter_*lodlights_small001.ymap` |
| 20 | Only `distlodlights_small001.ymap` shipped: its removal is made in the vanilla `lodlights_small001.ymap` through its pair document | `smpl_distant_only/stream/distlodlights_small001.ymap` | `partner` in `source-cache/implied/smpl_distant_only/distlodlights_small001.ymap.json` |
| 21 | A light added to one YMAP of the pair only: left out | `smpl_one_side_light/stream/lodlights_small001.ymap` | `skip` in `source-cache/implied/smpl_one_side_light/lodlights_small001.ymap.json` |

## YBN

| # | Example | Files | What to look at |
|---|---|---|---|
| 22 | The park's collision holds a box twice and is deployed once, though no resource changes it; an overwrite leaves the planter's collision out | vanilla `smpl_block_02.ybn`; `map/overwrite/resources/smpl_street_tweaks/smpl_block_01.ybn.diff.json` | `merged/report/ybn_repeats.json`; `merged/ybn/smpl_block_02.ybn`, `smpl_block_01.ybn` |
| 23 | Surface names: the vanilla archive's `materials.dat` lists the sample's surface types | `vanilla/materials.dat` | `vanilla-cache/materials.json`; `surface` in each material of `--read --all`'s YBN documents |

## Overwrites

| # | Example | Files | What to look at |
|---|---|---|---|
| 24 | One ignored overwrite of every reason on the first run, three of them written against earlier versions of the resources' files | `map/overwrite/`, `map/source-cache/history/` | `ignored_overwrites` in the run's report; `overwrite-cache/source_updates/`; the rewritten `map/overwrite/resources/smpl_street_tweaks/smpl_block_01.ymap.diff.json` |

The overwrites of 24:

| Overwrite | Reason | What it shows |
|---|---|---|
| `resources/smpl_street_tweaks/smpl_block_01.ymap.diff.json` | rewritten | Reverts the planter and a cone its earlier version added. The current version adds the planter only, so the run rewrites it against the current one: it keeps reverting the planter and no longer names the cone. |
| `resources/smpl_block_rework/maps/smpl_block_01.ymap.diff.json` | `source_changed` | Reverts the lamp's colour, which the current version changes to another colour; left to be written again. |
| `resources/smpl_park_tweaks/smpl_block_02.ymap.diff.json` | `source_changed` | Reverts the removal of the first oak and the bin its earlier version placed last; the current version removes the third oak and places the bin after the fountain, so it is left too, its record listing what it reverted. |
| `vanilla/0001-update/ymap/smpl_block_02.ymap.diff.json` | `vanilla_changed` | Written against the title update's park, which later stages replace. |
| `resources/smpl_custom_props/smpl_custom_old.ymap.diff.json` | `missing_source` | Reverts a sign in a map an earlier version of `smpl_custom_props` shipped and the current one drops. |
| `vanilla/0000-base/ymap/smpl_block_99.ymap.diff.json` | `missing_vanilla` | Names a file no stage of the vanilla archive has. |
| `resources/smpl_street_tweaks/smpl_props.ytyp.diff.json` | `unsupported_format` | Leaves out a YTYP; overwrites apply to YMAPs and YBNs only. |

The earlier versions are kept in the source cache's history, as a pipeline run on them would
have left them, with their diff documents. Read one with, for example,

```bash
mlo_merger --read --resource smpl_street_tweaks --version {sha256} smpl_block_01.ymap
```

where `{sha256}` names the file in the history.

## Store Layout

### Configuration (`config.toml`)

Default tolerances, YBN repeats left out, and the sample's own game builds: build 1 stops at
the title update, build 100 first includes `smpl_winter` and build 200 `smpl_summer`.

### Vanilla (`vanilla`)

| Stage | Build | Files |
|---|---|---|
| `0000-base` | 1 | The blocks `smpl_block_01.ymap` (one item of every kind) and `smpl_block_02.ymap` (the park), with `smpl_block_01_lod.ymap`; the LOD light pair `lodlights_small001.ymap` / `distlodlights_small001.ymap` (three lights); the square and its LOD map; the row and the quay with their LOD maps; the pier; the collisions `smpl_block_01.ybn`, `smpl_block_02.ybn` and `smpl_yard.ybn` |
| `0001-update` | 1 | `smpl_block_02.ymap` (an oak moved, another turned), the LOD light pair (a fourth light), `smpl_yard.ybn` (a bench) |
| `0002-smpl_winter` | 100 | `winter_smpl_block_01.ymap`, `winter_lodlights_small001.ymap`, `winter_distlodlights_small001.ymap`, `winter_smpl_square.ymap`: generations, items reordered, added and removed; `winter_smpl_rink.ymap`. Its title-update patches: `update_rpf/100` (the rink with a lamp) and `update_rpf/200` (the lamp moved and a bench; a hut in an undeclared archive) |
| `0003-smpl_patch_01` | 100 | `smpl_block_02.ymap` (a bench added) |
| `0004-smpl_summer` | 200 | `summer_smpl_block_01.ymap` and `summer_smpl_block_01.ybn` (generations), `smpl_block_02.ymap` (an oak swapped for a kiosk), `smpl_block_01.ybn` (the kiosk's collision), `summer_smpl_yard_0.ybn` and `summer_smpl_yard_1.ybn` (the yard split in two) |

`cache_info.json` records the archive holding each file and what each pack declares (`dlc`):

- `smpl_block.rpf` (the blocks, LOD maps and their collisions) and `smpl_lights.rpf` (the LOD
  light pair) are always loaded; `smpl_patch_01` and `smpl_summer` mount their own
  `smpl_block.rpf` over the base game's.
- `smpl_winter` replaces `smpl_square.rpf` with its own and invalidates `smpl_pier.rpf`;
  `smpl_summer` replaces `smpl_yard.rpf` with its own.
- `smpl_winter.rpf` and `smpl_summer.rpf` hold the packs' generations and the rink.
- `smpl_hut.rpf`, in the installed game's patch of `smpl_winter` only, is not declared by the
  pack and never loaded.

`materials.dat` names the surface types, and `hash_names.json` every hashed name of the sample.

### Resources (`map/source`)

| Resource | Example | What it ships |
|---|---|---|
| `[smpl]/smpl_street_tweaks` | 1, 6, 19, 22, 24 | `smpl_block_01.ymap`: moves the bench, locks the door, removes the car generator, adds a planter and extends the occlude model's wall east; the LOD light pair without its first light; the block's collision with the planter's |
| `smpl_block_rework` | 1, 6, 12, 24 | `maps/smpl_block_01.ymap`: brightens the time-cycle modifier, removes the occlude model's roof and the bin, extends the wall west, recolours the lamp; `maps/smpl_block_01_lod.ymap` without the empty SLOD; the collision without the bollard. The two wall strips pass the vertex limit of one occlude model, so the merge puts the second in a model of its own |
| `smpl_custom_props` | 17 | Its own map `smpl_custom_01.ymap`, a child map `smpl_custom_02.ymap` and the map's collision; a birch in the park; `smpl_summer`'s generation of the block's collision with a bench, carried to `smpl_block_01.ybn` under build 100 |
| `smpl_custom_props_v2` | 17 | `smpl_custom_01.ymap` again: a barrel first, the sign recoloured, the crate placed again under another GUID |
| `smpl_park_tweaks` | 16, 24 | `smpl_block_02.ymap`: removes the third oak, places a bin by the fountain |
| `smpl_summer_only` | 2 | `summer_smpl_block_01.ymap`: turns the flag, moves the parasol |
| `smpl_left_behind` | 1, 3 | `smpl_square.ymap`: moves the statue, removes the bench, turns the bin, adds a sign below the statue's LOD |
| `smpl_yard_split` | 4 | `smpl_yard.ybn`, made from the base game's: removes the post, adds a crate |
| `smpl_old_park` | 5 | `smpl_block_02.ymap`, made from the title update's: moves the first oak |
| `smpl_old_lights` | 7 | The LOD light pair, made from the base game's, its third light removed from `lodlights_small001.ymap` only |
| `smpl_vanilla_rink` | 8 | `winter_smpl_rink.ymap` as `smpl_winter` shipped it |
| `smpl_houses_gone` | 13 | `smpl_row.ymap` without the two houses below the SLOD |
| `smpl_shared_parent_guid` | 14 | `smpl_quay.ymap` with the crane turned |
| `smpl_orphaned_house` | 13, 15 | `smpl_row_lod.ymap` without the third house's LOD |
| `smpl_bench_moved`, `smpl_bench_moved_too` | 16 | `smpl_block_02.ymap`: the bench moved alike, the fountain turned each its own way; the second also turns the third oak |
| `smpl_square_both` | 16 | `smpl_square.ymap` and `winter_smpl_square.ymap`: the birch turned in each, the same flower bed in each |
| `smpl_rounded_guids` | 18 | `smpl_block_02.ymap` with every GUID rounded through a float and the kiosk turned |
| `smpl_distant_only` | 20 | `distlodlights_small001.ymap` without its second light |
| `smpl_one_side_light` | 21 | The LOD light pair with a light added to `lodlights_small001.ymap` only |

### A Position Every File Covers

Every stream file here, of every stage and resource and in the history, covers
**(-7090, 7092, 20.5)**: it lies inside each YMAP's entity extents (the sample takes each
archetype to reach 10 m from its entity) and each YBN's bounds. A file-from-position search
there lists every file the stores hold:

```bash
mlo_merger --find file-from-position -7090,7092,20.5 --all
```
