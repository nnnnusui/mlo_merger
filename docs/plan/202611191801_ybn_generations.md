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
- The YBN merge matches shapes by geometry within the coordinate tolerance, not by index,
  and merges only YBNs several resources ship; a YBN one resource ships is deployed as it is.

## Phases

### 1. Vanilla Cache: Loaded YBNs

- [ ] Apply the change sets to every archive holding YMAPs or YBNs, and list the YBNs of each
  archive and every YBN version in `dlc_activity.json`.
- [ ] Record the YBNs the game does not load in `generations.json`, with why.

### 2. Vanilla Cache: YBN Successors

- [ ] Group YBNs into generations and families by prefix and name, as YMAPs are, over the
  whole raw archive.
- [ ] Name the successor of each YBN not loaded: the YBN of the same name without its prefix
  in the loaded generation, or, when generations move shapes between files, the loaded YBN
  holding most of its shapes by geometry. Measure how often the plain name differs.
- [ ] Record the YBNs first shipped after the game build with a successor in `newer_files`.

### 3. Merge: Carrying Changes

- [ ] Merge every source YBN of a generation not loaded, with one source or several.
- [ ] Take each such source's removed and added shapes against its own vanilla generation, and
  apply them to the successor by geometry: a removed shape removes the shape matching it
  there, an added shape is added, a flag change applies to the matching shape.
- [ ] Merge the carried changes into the successor after its own sources, newest origin first,
  under the YBN policy, so a removal wins.
- [ ] Omit the YBNs with a successor, listing their sources in `_omit.txt`.

### 4. Report

- [ ] Report each YBN not deployed with its successor, and the removals that match no shape of
  the successor, in the merge report.

### 5. Documentation

- [ ] Describe YBN generations in `docs/reference.md`, `docs/ARCHITECTURE.md` and
  `docs/usage.md`, and the work left in `docs/TODO.md`.

## Decisions

- A streamed YBN of an invalidated base game archive loads nothing, but its changes are
  carried all the same, as a resource means them for the loaded generation.
