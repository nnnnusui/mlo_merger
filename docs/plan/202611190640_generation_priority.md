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
- On a game build without a pack, streaming that pack's YMAP loads it beside the loaded
  generation too: on build 3570, `m25_2_distlodlights_medium015` showed its lights beside
  `vw_`'s, as `hei_` and `apa_` did, while `vw_` alone replaced them.
- `mpheist4`'s LOD lights archive, holding only `island_lodlights` and its distant lights,
  invalidates the base game path as the others do, but leaves `vw_`'s loaded: on build 3570,
  streaming `vw_distlodlights_medium015` alone moved its light. An island hopper file of the
  pack names `island_lodlights` as the island's LOD lights. Its `content.xml`, `setup2.xml`
  and `dlclist.xml` are those of the build's own title update, which FiveM mounts for it.
- The latest title update's `lodlights.rpf` holds the `m25_2_` LOD lights; build 3570's holds
  the base game's.
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

- [x] Find the archives the game does not load by applying the recorded change sets, and
  write them with every YMAP version they hold to `dlc_activity.json` for tools to draw.
- [x] Pair entities of generations by GUID before position and archetype.
- [x] Record each YMAP not loaded with its loaded successor in `generations.json`.
- [x] Record the islands of island hopper files, and load the archives holding their LOD
  lights, and those `[dlc_activity] additive_archives` names, beside the archives they would
  otherwise replace.

### 3. Source Cache: Consistency of Each Source

- [x] Write what keeps each resource's LOD light pairs in step to `implied/`, leaving the diff
  documents as they are: the removals the other YMAP of a pair needs, as a diff, the
  resource's own other YMAP or vanilla's, and the additions made to one YMAP only, as a
  `skip`.

### 4. Merge: Carrying Changes

- [x] Resolve generations when merging, from the vanilla cache's generations and the source
  cache's diff and `implied/` documents, leaving the source cache free of generations.
- [x] Carry removals, modifications and replacements, after step 3, to the paired items of
  the newer loaded generations, including YMAPs the resource ships itself, newest origin
  generation first.
- [x] Carry entity additions without a parent to the successor, with the physics
  dictionaries they add, and LOD light additions to both YMAPs of the successor pair.
- [x] Leave out an addition a resource makes to an older generation when the same resource
  adds the same item to its successor, or to a newer generation carried there before: an
  entity by its GUID when it is not 0 and unique in its YMAP, else by type, archetype, LOD
  level and position within the matching tolerances, as `compare::added` thins the
  versions of an added YMAP; other items by equality within the tolerances. Additions only
  the older generation makes are carried.
- [x] Leave out, of all sources' entity additions to a vanilla YMAP, own and carried, those an
  earlier source makes already, by the same matching, and report them.
- [x] Record each change carried, already met in the loaded generation (an item it lacks
  removed), or lost (a modification of an item it lacks, an addition that cannot be carried).

### 5. Generations Newer Than the Game Build

- [x] Pair and record the generations of the whole raw archive, not only those of the
  selected game build.
- [x] Carry the changes resources make to YMAPs of generations newer than the game build
  back to the generation the build loads.
- [x] Find whether streaming such a YMAP on an older build loads it: it does, beside the
  loaded generation.
- [x] Omit such YMAPs from the merge, as YMAPs with a successor are.
- [x] Verify on build 3570 with a merge of a resource streaming a changed newer YMAP: the
  `m25_2_` source is omitted and its changes reach `vw_`.
- [x] Check in game on build 3570 that the carried lights show once, at the carried position.

### 6. Merge: YMAPs Not Loaded

- [x] Plan no output for a YMAP with a successor; omit its sources.

### 7. Merge Report

- [x] Write `merged/report/index.json` and `merged/report/vanilla/{version}/ymap/{file}.json`
  in place of `duplicates.json`: each entity competed for names the applied and ignored
  changes by resource and stream path or carried document, with its index in that diff and
  a reason.
- [x] Report each YMAP not deployed with its successor, and each change carried nowhere; the
  units pairing it with the successor are in `generations.json`.
- [ ] Report the other arrays' competing changes as entities' are.

### 8. Documentation

- [ ] Describe generation resolution in `docs/ARCHITECTURE.md`, `docs/usage.md` and
  `docs/reference.md`.

## Decisions

- A LOD light added to one YMAP of a pair only is dropped, not completed in the other: a
  distant light lacks the direction, falloff and flags of a LOD light, and a LOD light lacks
  the position and colour of a distant light.
- Additions to a YMAP with a successor that cannot be carried (entities with a parent, other
  arrays) are reported and dropped. Deploying that YMAP for them would load its generation
  again.
- Vanilla overwrites of YMAPs with a successor are not applied.
- A YMAP the game does not load without a successor is merged and deployed as before.
- Extents of the loaded YMAP stay vanilla's, as for every merged vanilla YMAP.
- Change sets of `GROUP_ON_DEMAND` count as enabling archives, since a script may run them,
  but not as invalidating any.
- An archive built before DLC content was recorded loads every YMAP.
