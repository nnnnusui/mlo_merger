# Game Data

How the installed game and FiveM lay out the archives that vanilla generation reads, and which
of them hold map data.

## Archives Of The Installed Game

| Archive | Contents |
|---|---|
| `x64*.rpf`, `common.rpf` | The base game: levels, models and data as first released |
| `update/update.rpf` | The title update: replacements of base game files, `common/data/dlclist.xml`, and `dlc_patch/{pack}/` with each DLC pack's patched `setup2.xml`, `content.xml` and archives |
| `update/update2.rpf` | Text (`x64/data/lang`), scripts (`x64/levels/gta5/*.ysc`), UGC data (`common/data/ugc`) and shaders; it holds no YMAP or YBN |
| `update/x64/dlcpacks/{pack}/dlc.rpf` | A DLC pack, with the subpacks its `setup2.xml` declares |

Vanilla generation reads the base game, `update/update.rpf` and the DLC packs, and, with
`--game-storage`, the `dlc_patch/` of each FiveM game build's `update/update.rpf` (see
[FiveM Game Builds](#fivem-game-builds)).
`update/update2.rpf` holds no map data, so it is not read.

The collision surface types YBN materials index are listed in `common/data/materials/materials.dat`,
which the title update replaces. The title update's file is the same for builds 3095 to 3889;
it inserts `STUNT_RAMP_SURFACE` in the middle of the base game's list, so the surface types
after it are one index later than in `common.rpf`. `mpstunt`'s `dlc.rpf` holds another copy,
which its `content.xml` does not declare.

## FiveM Game Builds

FiveM keeps each game build's `GTA5.exe`, `update/update.rpf` and `update/update2.rpf`
(`g_entriesToLoadPerBuild` in `code/client/launcher/GameCache.cpp`); DLC packs are the
installed game's for every build. It downloads them into `FiveM.app/data/game-storage/`,
named `{path with / as +}_{sha1}`, such as `update+update.rpf_{sha1}`.

How a client runs a build below the latest stable one depends on the server's
`sv_replaceExeToSwitchBuilds`
([server commands](https://docs.fivem.net/docs/server-manual/server-commands/)):

| Value | Client | Default |
|---|---|---|
| `true` | Downloads the build's files and runs the build's own executable | Server builds below 12872 |
| `false` | Runs the latest stable executable with the installed `update/update.rpf`, and mounts the build's own `update.rpf` as `override+update+update.rpf_{sha1}` over the paths below | Server builds from 12872 |

With `sv_enforceGameBuild 1`, the client runs as with `false`. Either way the client loads the
build's own `dlc_patch/` and `dlclist.xml`, and the DLC packs that list names, so the map data
is the build's.
With `false`, the paths taken from the build's `update.rpf` are these
(`code/components/rage-device-five/src/UpdateRpfOverrideMount.cpp`):

| Device | Paths taken from the build's `update.rpf` |
|---|---|
| `update:/` | `dlc_patch/` (the whole directory), `content.xml`, `setup2.xml`, `common/data/ExtraTitleUpdateData.meta`, `x64/patch/` |
| `common:/` | `data/dlclist.xml` |
| `platform:/` | `levels/gta5/lodlights.rpf`, `levels/gta5/paths.rpf`, `levels/gta5/scenario/`, `levels/gta5/sp_manifest.` |

Every other file, the base game replacements of the title update included, is the installed
build's.

## Differences Between Builds

Between the `update/update.rpf` of build 3570 and that of build 3889:

- The YMAPs and YBNs that replace base game files are the same.
- `dlc_patch/` differs: build 3889 adds archives to the patches of `mpheist` (523 YMAPs and
  YBNs) and `mpapartment` (497), with `content.xml` declaring them, and adds the patch of
  `mp2025_02`, a pack after build 3570. Most of the files added to `mpheist` and
  `mpapartment` (344 and 417) are new versions of files the pack's own `dlc.rpf` holds,
  nearly all with other bytes; the rest (179 and 80) the pack does not hold.
- `dlclist.xml` differs only by the packs build 3889 appends.
- `lodlights.rpf`, `paths.rpf`, `content.xml` and `setup2.xml` are the same; `scenario/`,
  `sp_manifest`, `ExtraTitleUpdateData.meta` and `x64/patch/` differ in files other than YMAPs
  and YBNs.

Across the title updates of builds 3095 to 3889 (one file each for 3095 and 3179, 3258, 3407,
3570, 3717, 3751 and 3788, and 3889; 3323's was not compared), the YMAPs and YBNs differ only
in `dlc_patch/`:

| Builds | YMAPs and YBNs of `dlc_patch/` |
|---|---|
| 3095 to 3570 | The same |
| 3717 | Adds the `mpheist` and `mpapartment` archives above, 75 of their files in versions 3751 replaces |
| 3751 and 3788, 3889 | The same as each other: 3717's with those 75 replaced, and the `mp2025_02` patch |

Of the packs' patches, only those of `mpheist` and `mpapartment` hold other YMAPs and YBNs
on one build than on another; the others differ in what the packs declare, or in files
other than YMAPs, YBNs and what the packs declare, or are missing from the builds before
their pack's patch was added.

So for a build below the installed one, the map data FiveM loads differs from the installed
game's in the DLC packs' `dlc_patch/`: a file a later title update adds to a pack's patch is
loaded from the pack's `dlc.rpf` instead, or is missing when the pack does not hold it, and the
pack's `content.xml` is the build's own.

A later patch can also move entities between YMAPs. Build 3889 reworks `hei_ch1_09` of
`mpheist` for script-loaded content:

- 245 entities that its YMAPs loaded with the map (plants, rocks, chairs) move into
  script-loaded YMAPs, such as `hei_ch1_09_props_original` and `hei_ch1_09_mansion_shared`.
- The rest are numbered anew: `hei_ch1_09_strm_5` of build 3889 holds what `hei_ch1_09_strm_6`
  holds in the pack's `dlc.rpf`, and shares no entity with the `hei_ch1_09_strm_5` there.

A YMAP made from one build's version therefore removes or repeats entities on the other build.

A version from a later patch can differ only in its parent indices, because its parent YMAP
was reordered in the same patch. `hei_ch1_06f_strm_0.ymap` of `mpheist` is one: the version of
the build 3889 patch and the one in the pack's `dlc.rpf` hold the same entities, their parent
indices one or two apart. Streaming either version over a build 3570 client replaces the
loaded one alike, so an in-game check by entity positions does not tell them apart.

## Script-Loaded YMAPs

A YMAP with bit 0 of `flags` set is loaded only when a script requests it (`RequestIpl`); the
map does not load it by itself. Streaming such a YMAP under a name no loaded YMAP has adds its
entities once a script requests it, on top of the entities the loaded YMAPs place at the same
spots, whichever build the client runs. `hei_ch1_09_props_original` of the build 3889 patch
behaves alike on builds 3570 and 3889: neither loads it, and requesting it repeats the plants
and rocks that build 3570 loads with `hei_ch1_09` and that build 3889 loads with
`hei_ch1_09_mansion_shared`, when a script requests that one too.

In build 3889, parts of the hills mansions, their terrain included, are script-loaded: without
a script requesting them, such as the IPL loader resource `bob74_ipl`, the ground there is
missing.
