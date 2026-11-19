# Generation Resolution Plan

## Goal

Deploy, of every YMAP family, only the generation the game loads for the selected game build,
with the changes resources make to the other generations carried over to it. Report what the
merge cannot carry, or ignores in favour of another change, per vanilla YMAP for tools to draw.

## Findings

- Which YMAPs the game loads follows each DLC pack's `content.xml` change sets, applied in
  stage order: a change set invalidating a base game path unloads the archives earlier packs
  enabled in its place, so of the packs replacing one archive in turn (the LOD lights of
  `hei_`, `apa_`, `vw_` and `m25_2_`, or `hei_` and `lr_` in `scentral_01`) only the last
  one's is loaded. An archive no later pack replaces stays loaded, whatever its generation
  (`hei_vb_rd_critical_2`).
- The title update replaces some packs' `content.xml` and `setup2.xml`
  (`update.rpf/dlc_patch/{pack}/`), as for `mp2025_02`'s LOD lights.
- Streaming a YMAP of an archive a later pack replaced loads it again beside the loaded one,
  placing its items twice. A YMAP of an invalidated base game archive stays unloaded when
  streamed.
- The successors found from the change sets match those found by pairing items for all but
  four YMAPs, LOD lights no generation of which is loaded.
- Pairing entities by GUID first, then by archetype and position, leaves 1418 older entities
  unpaired across all units, mostly removed by the newer generation, and 29 replaced at the
  same position by another model.

## Rules

For each vanilla item of a YMAP the game does not load, with the item it pairs with in the
loaded generation:

| Older change | Loaded change | Result in the loaded generation |
|---|---|---|
| Removed | any | Removed |
| any | Removed | Removed |
| Modified | none | Modified (older) |
| none | Modified | Modified (loaded) |
| Modified | Modified | Modified (loaded) |
| none | none | vanilla |

Between carried modifications, the newer origin generation wins.

## Phases

### 1. Raw Archive: DLC Content

- [x] Record each DLC stage's device, data files and the change sets the game executes, from
  the title update's replacements of `setup2.xml` and `content.xml` where present.

### 2. Vanilla Cache: Loaded YMAPs and Successors

- [ ] Find the latest YMAPs the game does not load by applying the recorded change sets.
- [ ] Pair entities of generations by GUID before position and archetype.
- [ ] Record each YMAP not loaded with its loaded successor in `generations.json`.

### 3. Source Cache: LOD Light Pairs

- [ ] Keep each resource's LOD light pair diffs in step: a light removed from either YMAP is
  removed from both, and a light added to one YMAP is kept only when the other adds one at
  the same index.
- [ ] Write the removals resources shipping one YMAP of a pair imply for the vanilla other
  one as documents of their own.

### 4. Source Cache: Carrying Changes

- [ ] Carry removals, modifications and replacements, after step 3, to the paired items of
  the newer loaded generations, including YMAPs the resource ships itself, split per origin
  generation.
- [ ] Carry entity additions without a parent to the successor, with the physics
  dictionaries they add, and LOD light additions to both YMAPs of the successor pair.
- [ ] Record each change carried, already met in the loaded generation (an item it lacks
  removed), or lost (a modification of an item it lacks, an addition that cannot be carried).

### 5. Source Cache: Generations Newer Than the Game Build

- [ ] Pair and record the generations of the whole raw archive, not only those of the
  selected game build.
- [ ] Carry the changes resources make to YMAPs of generations newer than the game build
  back to the generation the build loads.
- [ ] Find whether streaming such a YMAP on an older build loads it, and deploy or omit it
  accordingly.

### 6. Merge

- [ ] Merge the documents of steps 3 to 5 as resource diffs, newest origin first, after the
  loaded YMAP's own diffs.
- [ ] Plan no output for a YMAP with a successor; omit its sources.

### 7. Merge Report

- [ ] Write `merged/report/index.json` and `merged/report/vanilla/{version}/ymap/{file}.json`
  in place of `duplicates.json`: each item names the applied and ignored contributions by
  resource and stream path, with array, `base_index` and a reason.
- [ ] Report each YMAP not deployed with its successor and units, and each change lost.

### 8. Documentation

- [ ] Describe generation resolution in `docs/ARCHITECTURE.md`, `docs/usage.md` and
  `docs/reference.md`.

## Decisions

- Additions to a YMAP with a successor that cannot be carried (entities with a parent, other
  arrays) are reported and dropped. Deploying that YMAP for them would load its generation
  again.
- Vanilla overwrites of YMAPs with a successor are not applied.
- A YMAP the game does not load without a successor is merged and deployed as before.
- Extents of the loaded YMAP stay vanilla's, as for every merged vanilla YMAP.
- Change sets of `GROUP_ON_DEMAND` count as enabling archives, since a script may run them,
  but not as invalidating any.
- An archive built before DLC content was recorded loads every YMAP.
