# YBN Generations Plan

## Goal

Deploy, of every YBN DLCs ship again under a prefix, only the generation the game loads, with
the changes resources make to the others carried over to it by geometry, as YMAP generations
are resolved.

## Findings

- 1240 vanilla YBNs have older generations under a prefix with the same name: 965 of them
  `hei_` only, 120 `hei_` and `lr_`, 71 `lr_`, 61 `apa_` and `hei_`. Their archives
  (`levels/gta5/.../{area}.rpf`) are invalidated and enabled by the same kind of change sets
  as YMAP archives, but hold no YMAP, so `dlc_activity.json` does not list them.
- On the latest build, YBNs load as YMAPs do (see the
  [generation resolution plan](202611190640_done_generation_priority.md)):
  - Clearing the collision flags of the loaded generation's YBN (`hei_dt1_22_0`,
    `lr_sc1_12_0`) removes its collisions.
  - Streaming the replaced `hei_sc1_12_0.ybn` beside it brings its collisions back: a
    replaced generation loads beside the loaded one.
  - Streaming the invalidated base game `dt1_22_0.ybn` or `sc1_12_0.ybn` loads nothing.
  - Clearing an older generation's collisions alone changes nothing.
- YBNs diff as the polygons of the whole file grouped by child collision flags, matched by
  geometry within `ybn_position` ([plan](202611261136_done_ybn_polygon_diff.md)), so a change
  needs no item pairing to reach another generation: a removed polygon is found in the
  successor by its geometry, and an added one is added there.
- A YBN merges when several resources replace it or an overwrite skips within it; one only
  one source replaces is deployed as that source has it.

## Design

- **Loaded YBNs.** The vanilla cache applies the change sets to every archive holding YMAPs or
  YBNs, as for YMAPs, and lists the YBNs each archive holds; `generations.json` records the
  YBNs the game does not load in `inactive`.
- **Families.** YBNs form generations and families by prefix and name, as YMAPs do, over
  their own names. No units are built: polygons match by geometry.
- **Successor.** A YBN the game does not load has as successor the loaded YBN of its stem in the
  newest newer generation of its family, or, for one first shipped after the game build, the
  newest older one.
- **Carrying.** A source YBN of a generation not loaded is not deployed. Its diff, taken against
  its own vanilla generation and after its overwrite, is carried to the successor: a removal of
  a polygon the successor holds (by geometry, in the same flags group) is carried, the others
  are left behind as `already_absent`; additions are all carried. The carried changes are
  written to `generations/{resource}/{version}/{file name}.diff.json` and merged into the
  successor as one more source, newest origin first, after its own sources.

## Phases

### 1. Vanilla Cache: Loaded YBNs

- [ ] List YBNs in `dlc_activity.json` beside YMAPs: each archive's latest YBNs, and every
  YBN version in `files`.
- [ ] Record the YBNs the game does not load in `generations.json` `inactive`.
- [ ] Test: a YBN of an invalidated base game archive is inactive, the pack's replacing one is
  not.

### 2. Vanilla Cache: YBN Families And Successors

- [ ] Group YBNs into generations and families by prefix and name, as YMAPs are.
- [ ] Name the successor of each YBN not loaded, as the design says. Measure on the local vanilla
  archive how many have none.
- [ ] Bump the generation and activity layouts.

### 3. Merge: Carrying Changes

- [ ] Merge every source YBN of a generation not loaded, its changes carried to the successor:
  write the carried documents (`YbnPropagatedDiff`), list the removals the successor lacks in
  `left_behind.json`, and omit the source.
- [ ] Merge the carried documents into the successor after its own sources, newest origin first.
- [ ] Tests: a removal and an addition carried; a removal the successor lacks left behind; the
  source omitted.

### 4. Samples And Documentation

- [ ] Give the web sample a prefixed YBN generation whose changes carry over.
- [ ] Describe YBN generations in `docs/reference.md`, `docs/ARCHITECTURE.md` and
  `docs/usage.md`, and the work left in `docs/TODO.md`.

## Decisions

- A streamed YBN of an invalidated base game archive loads nothing, but its changes are
  carried all the same, as a resource means them for the loaded generation.
