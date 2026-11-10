# Web Demo Sample

The workspace the page in `web/` seeds before running the merger. Everything is original: names
are invented (`smpl_*`, the DLCs `smpl_winter` and `smpl_summer`, and the game builds 100 and
200), and everything sits around (-7100, 7100), in the sea off the map, so no game data is
shipped. `src/core/web_sample` builds every file here; regenerate them with

```bash
MLO_WRITE_SAMPLES=1 cargo test --lib web_sample
```

and its tests check that the files are current and that the pipeline runs on them.
`files.json` lists the files the page seeds.

## Configuration (`asset/config.toml`)

Default tolerances and the sample's own game builds: build 100 first includes `smpl_winter`,
build 200 `smpl_summer`, and build 1 stops at the title update.

## Vanilla (`asset/vanilla`)

| Stage | Build | Files |
|---|---|---|
| `0000-base` | 1 | `smpl_block_01.ymap`, its parent `smpl_block_01_lod.ymap`, `smpl_block_02.ymap`, the LOD light pair `lodlights_small001.ymap` / `distlodlights_small001.ymap`, `smpl_block_01.ybn` |
| `0001-update` | 1 | `smpl_block_02.ymap` (a tree moved) |
| `0002-smpl_winter` | 100 | `winter_smpl_block_01.ymap`, `winter_lodlights_small001.ymap`, `winter_distlodlights_small001.ymap`: DLC generations, items reordered and added |
| `0003-smpl_patch_01` | 100 | `smpl_block_02.ymap` (a bench added) |
| `0004-smpl_summer` | 200 | `summer_smpl_block_01.ymap` (another generation), `smpl_block_02.ymap` (a tree swapped for a kiosk), `smpl_block_01.ybn` (the kiosk's collision) |

`smpl_block_01.ymap` holds one item of every kind: entities with a door, light effect, spawn
point override and cloth bounds extension, a car generator, a time-cycle modifier, a box
occluder, an occlude model, a physics dictionary and a grass batch. Its houses point to LOD
entities of `smpl_block_01_lod.ymap`. `smpl_block_01.ybn` has a geometry of triangles, a box
and a sphere, and a box bound. The LOD light files keep the `lodlights_` and `distlodlights_`
names, by which the merger pairs them.

## Resources (`asset/source`)

- `[smpl]/smpl_street_tweaks` moves the bench, locks the door, removes the car generator and
  adds a planter in `smpl_block_01.ymap`; removes a LOD light pair; adds the planter's
  collision.
- `smpl_block_rework` brightens the time-cycle modifier, removes the occlude model's roof and
  the bin, recolours the lamp; removes the unused SLOD from `smpl_block_01_lod.ymap`, so the
  houses' parent indices move; removes the bollard's collision.
- `smpl_custom_props` ships its own map `smpl_custom_01.ymap` (a sign and a crate), a child map
  `smpl_custom_02.ymap` whose label points at the crate, and the map's collision, and adds a
  birch to the park as `smpl_summer` left it.
- `smpl_custom_props_v2`, a later version of it, ships `smpl_custom_01.ymap` again: a barrel
  first, the sign recoloured, and the crate placed again under another GUID and turned. No
  vanilla YMAP has that name, so the merge combines both versions, the one loaded last
  (`smpl_custom_props_v2`) first: the sign, by its GUID, and the crate, by its archetype, LOD
  level and position, are kept once, from `smpl_custom_props_v2`, and the label's parent index
  follows the crate it kept.

Edits to `smpl_block_01.ymap` and the LOD lights carry over to their DLC generations. The
resources' collisions predate `smpl_summer`, so merging against the last stage removes the
kiosk's collision, while `--gamebuild 100` merges against the stage they were made for.

## Overwrites Written Against Earlier Versions (`asset/overwrite`, `asset/source-cache/history`)

Two overwrites were written against earlier versions of the resources' `smpl_block_01.ymap`,
which the source cache keeps under `asset/source-cache/history/`, as a pipeline run on them
would have left them, with their diff documents:

- `smpl_street_tweaks`'s reverts the planter and a cone its earlier version added. The current
  version no longer adds the cone, so the first pipeline run rewrites the overwrite in
  `asset/overwrite` against it: it keeps reverting the planter and no longer names the cone.
- `smpl_block_rework`'s reverts the lamp's colour, which the current version changes to
  another colour. It is left as it is, not applied, and listed in `ignored_overwrites`, to be
  written again.

Both are recorded under `asset/overwrite-cache/source_updates/`. Read an earlier version with,
for example,

```bash
mlo_merger --read --resource smpl_street_tweaks --version {sha256} smpl_block_01.ymap
```

where `{sha256}` names the file in the history.

## A Position Every File Covers

Every stream file here, YMAP or YBN, of every stage and resource and in the history, covers
**(-7090, 7092, 20.5)**: it lies inside each YMAP's entity extents (the sample takes each
archetype to reach 10 m from its entity) and each YBN's bounds. A file-from-position search
there lists every file the stores hold:

```bash
mlo_merger --find file-from-position -7090,7092,20.5 --all
```
