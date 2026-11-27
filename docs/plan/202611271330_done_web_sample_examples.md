# Web Sample Examples Plan

## Goal

Reorganize `web/sample` so that it shows, file by file, each behaviour of the merger a user meets
on real data, with the generations of a stream file and resources made from an older version of
a vanilla file as two of them. Keep it original, small and within one position every file
covers.

## Findings

- The sample already shows: YMAP generations (`winter_` / `summer_smpl_block_01.ymap`), a YBN
  generation (`summer_smpl_block_01.ybn`), a LOD light pair and its generation, an added map
  shipped by two versions of a resource, a child map of it, parent index repair after a LOD
  entity is removed, an occlude model strip past the vertex limit, a YBN holding a box twice,
  every reason an overwrite is ignored, and game builds 100 and 200 selecting stages.
- Resources made from an older vanilla version appear only by accident: the collisions predate
  `smpl_summer`, which the README mentions in one sentence. No YMAP resource is made from an
  older version on purpose.
- The README lists files by store, not by what they show, and lags the module docs: it does not
  mention `summer_smpl_block_01.ybn`, `smpl_block_02.ybn` or the occlude model strips.
- The vanilla archive records no DLC content (`dlc`), no title-update patch versions
  (`patches`, `title_updates`) and no `materials.dat`, so `dlc_activity.json`, per-build patches
  and YBN surface names never show.
- The merge report shows some reasons, but not each of `identical_duplicate`, `removal_wins`,
  `newer_generation_wins`, `earlier_source_wins` and `same_addition_newer` on purpose, and
  `left_behind` shows polygons only.

## Examples

Each example names the files it adds or changes and what to look at after a pipeline run.
Existing ones are marked as kept.

### Generations

1. Kept: `smpl_block_01.ymap` with `winter_` and `summer_` generations, edits carried to the
   loaded one, `_omit.txt` and `superseded_by`.
2. A YMAP first shipped after build 100 (`summer_` only): with `--gamebuild 100` it is listed in
   `newer_than_build`, and a resource streaming it is diffed against it.
3. `left_behind` with each reason: a removal of an item the loaded generation lacks
   (`already_absent`), a change to an item it does not pair (`no_loaded_counterpart`), and an
   entity added with a parent to a YMAP with a successor (`addition_not_carried`).
4. YBN candidates: `smpl_summer` splits the block's collision into two YBNs of other names, so a
   removal goes to both candidates holding the polygon and an addition to the one covering it.

### Older Versions Of A Vanilla File (Source Baselines)

5. A resource made from the title update's park (`0001-update` `smpl_block_02.ymap`): it moves
   one tree and keeps the rest. Its diff takes `0001-update` as `baseline`, leaves out the
   bench `smpl_patch_01` added and the tree `smpl_summer` swapped for the kiosk, and keeps the
   move.
6. Kept, explained: the resources' collisions made before the kiosk, whose YBN baseline keeps
   the kiosk's collision.
7. A LOD light pair made from the base version, before `smpl_winter`'s added light: both YMAPs
   keep the later light, and a light removed from one side only is removed from both.
8. A resource matching an older version exactly: its diff is empty, and nothing is deployed.

### Game Builds And DLC Content

9. Per-build title-update patches: `smpl_winter` gets a `dlc_patch` version for build 100 and
   another for build 200 (`update_rpf/100`, `update_rpf/200`), so `--gamebuild 100` and the
   latest read different versions of one YMAP, and `title_updates` lists the builds.
10. DLC activity: `smpl_winter` records a change set invalidating a base archive, so that
    archive's YMAP is listed in `inactive` and stands for nothing; an archive the patch holds
    without the pack declaring it shows `not_declared`.
11. Kept: builds 1, 100 and 200 ending at different stages.

### Entity Parents

12. Kept: parent indices repaired after a LOD entity is removed.
13. A resource removing every house of a block: their LOD entity loses all its children and is
    removed, and its SLOD in turn.
14. An entity whose parent GUID is shared by two entities: the reference is kept by index.
15. A child whose parent the resource removes: detached, `LODTYPES_DEPTH_ORPHANHD`, with a
    warning.

### Combining Sources

16. The merge report with one competed entity or item for each reason: `identical_duplicate`
    (two resources moving the bench alike), `removal_wins`, `earlier_source_wins`,
    `newer_generation_wins` and `same_addition_newer`.
17. Kept: an added map shipped by two versions of a resource, combined.
18. Rounded GUIDs: a resource whose tool rounded entity GUIDs through a float, paired with
    vanilla's.

### LOD Light Pairs

19. Kept: a pair removed by one resource.
20. A resource shipping only `distlodlights_small001.ymap`: its removals are made in the vanilla
    `lodlights_small001.ymap` through its LOD light pair document.
21. A light added to one YMAP of a pair only: left out and recorded in the pair document.

### YBN

22. Kept: a box held twice, deployed once; the planter's collision left out by an overwrite.
23. Surface names: the vanilla archive holds an original `materials.dat` listing the sample's
    surface types, so `--read` writes `surface` for each material.

### Overwrites

24. Kept: one ignored overwrite of every reason, and those written against earlier versions.

## Layout

- The README is grouped by the sections above, each a table of the example, its files and what
  to look at, then the store layout (stages, resources, overwrites) as a reference. The module
  docs of `src/app/map/web_sample` give the same list briefly and point to the README.
- Each example's resource is named after what it shows (`smpl_old_park`, `smpl_houses_gone`,
  `smpl_distant_only`), so `files.json` and the web page read as a list of examples.
- Every new file covers `EVERY_FILE_POSITION`, and names stay invented.
- `tests.rs` checks each example's outcome with one assertion block per example, named after
  it, rather than one long pipeline test.

## Steps

1. Restructure the README and module docs by the sections above, for the kept examples only.
2. Split the pipeline test into per-example assertions, unchanged in what they check.
3. Add the older-version examples (5–8).
4. Add the generation examples (2–4).
5. Add per-build patches and DLC content (9–10): the sample archive's `cache_info.json` gains
   `patches`, `title_updates` and `dlc` records.
6. Add the entity parent, combining and LOD light pair examples (13–21).
7. Add `materials.dat` (23).
8. Regenerate with `MLO_WRITE_SAMPLES=1 cargo test --lib web_sample` after each step, and check
   the web page loads the sample.

## Decisions

- The page loads the whole sample at once; `files.json` stays a flat list, and examples are
  told apart by their resources' names. The sample grew from 52 files (189 KB) to 105 files
  (255 KB).
- Every value is invented for the sample; nothing is copied from game files.
- The vanilla archive records an archive per area below `levels/smpl/` and what each pack
  declares (`dlc`), so DLC activity applies: `smpl_winter` replaces the square's archive and
  invalidates the pier's, and `smpl_summer` replaces the yard's. The block and the LOD lights
  stay loaded, so the kept examples keep their outcomes.
- 1 and 3 use the square, which `smpl_winter` supersedes: the block's generations are all loaded,
  so it shows no `superseded_by`.
- 4 uses a yard of its own rather than the block's collision, which 6 needs as `smpl_summer`
  changes it under its own name.
- 7 relates to the title update's fourth light, as the winter lights are another file. Only the
  first three lights are street lights, so removing one changes `num_street_lights` and each
  resource's baseline is told without a tie.
- 8 uses `smpl_winter`'s rink rather than the park: the park has other sources, so its file is
  omitted anyway, while the rink shows that a source made from an older version is not deployed.
- 5's title update also turns the second oak, so the old park differs least from it rather than
  tying with the base game's.
- The park's GUIDs lie far apart, so 18's rounding gives each its own GUID.
- `addition_not_carried` is shown by an entity added below the statue's LOD, as the plan says.

## Findings While Building It

- The sample wrote YBN vertices as world positions, which the XML format takes relative to
  `GeometryCenter`, so every collision read back at twice its position. Fixed in the builder.
- A YBN with a successor and one source was deployed as it is: single-source YBNs left the
  merge before superseded ones were set aside. They now stay for the successor check.
- A source made from an older vanilla version was deployed as it is whenever the merge left the
  vanilla file unchanged, and a single-source YBN of one was written as the source has it, both
  reverting what vanilla changed since. Such a source is now merged from its differences, and
  omitted and not deployed when that leaves vanilla's file (`made_from_older` in the merge's
  `cache_info.json`).

## Open Questions

- Whether the web page should let a reader pick one example and see only its files, which
  needs `files.json` to group files by example.
- The browser build was not run here (no wasm toolchain); `tests/host_layout.rs` seeds the same
  files natively.
