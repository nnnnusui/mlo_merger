# YBN Generation Candidates Plan

## Goal

Carry the changes resources make to a YBN the game does not load to the loaded YBNs that cover
its polygons, chosen polygon by polygon among candidates the vanilla cache records, rather than
to one successor of the same name. Write YMAP and YBN families to files of their own.

## Findings

- A YBN's number is the index of a spatial piece of its area, not an identity: a DLC rebuilding
  an area splits its collision again. The base game's seven `{area}_0`–`_6` became four
  `hei_{area}_0`–`_3` covering other ranges, and the rest of the area went to YBNs of other
  names (`hei_{area}_{part}_1`–`_3`).
- Of the 2586 successors named by the same unprefixed name on the local vanilla archive, 2451
  overlap their YBN by 90% or more (intersection over union of the XY boxes); 135, in 22
  families, do not, and 111 of those do not overlap it at all. Removals carried there are left
  behind, and additions go to a YBN covering another range.
- The marker before `@` (`hi@`, `ma@`) names a kind of collision, and generation prefixes follow
  it (`hi@hei_{area}_0`):
  - Unmarked YBNs hold the map collision of several flag pairs.
  - `hi@` YBNs hold only `MAP_WEAPON` children colliding with `PROJECTILE`, `TEST_WEAPON` and
    `TEST_CAMERA`: detailed collision for weapons and cameras, with other shapes and ranges.
  - `ma@` YBNs hold a few flat triangles whose children have no flags, with procedural ids and
    material colours; what they are for is unknown.
- `dlc_activity.json` records, for an archive a change set enables in place of a base game
  path, the path it stands for (`stands_for`).
- Root bounds of vanilla YBNs are in world coordinates, and the bound tree reads without
  geometry (`read_ybn_bound_tree`), cheaply.

## Design

- **Files.** `generations.json` keeps what does not depend on the file type: the stage it was
  built for, and the files the game does not load (`inactive`) or that came after the build
  (`newer_than_build`). The YMAP families move to `ymap_generations.json`
  (`YmapGenerationIndex`), and the YBN families and candidates to `ybn_generations.json`
  (`YbnGenerationIndex`).
- **Candidates.** For each YBN the game does not load, `ybn_generations.json` lists as
  candidates the loaded YBNs of the same marker whose XY box overlaps its box, from the
  archives standing for its archive's path, or, for a YBN first shipped after the game build,
  from any loaded archive; each with its box and the share of the YBN's box it covers. A YBN
  with no candidate is listed with none.
- **Families.** YBN families keep their generations by prefix and name, for reading; carrying
  goes by candidates only.
- **Carrying.** A removal is carried to every candidate holding the polygon (by geometry, in
  the same flags group), and left behind when none does. An addition goes to the candidate
  whose box holds the polygon's box, of those the one covering most of the YBN's box; when
  none holds it, to the candidate whose box is nearest the polygon. Whole-child removals are
  carried to every candidate holding the child. The carried changes of one source YBN are
  written per candidate, to `generations/{resource}/{version}/{file name}/{candidate}.diff.json`,
  so that two YBNs of one resource carried to one candidate stay apart, and merge as now.

## Phases

### 1. Separate Files

- [x] Move the YMAP families to `ymap_generations.json` and the YBN families to
  `ybn_generations.json`, keeping `inactive`, `newer_than_build` and the stage in
  `generations.json`; bump the layouts.
- [x] Add `YmapGenerationIndex` and `YbnGenerationIndex` beside `GenerationIndex`; move readers
  (merge, check, web sample tests) to the file they need.

### 2. Candidates

- [x] Read the bound tree of every YBN the game does not load and of the loaded YBNs of the
  archives standing for its archive's path, and list the overlapping ones of its marker as
  candidates.
- [x] Test: a YBN split into two by a newer generation names both; a neighbouring loaded YBN
  of another archive is not named; a `hi@` YBN names only `hi@` candidates.
- [x] Measure on the local vanilla archive how many YBNs have no candidate, and compare with
  the same-name successors.

### 3. Carrying By Polygon

- [x] Carry each source YBN's removals and additions to its candidates as the design says, and
  list removals no candidate holds in `left_behind.json`.
- [x] Tests: a removal found in the second candidate; an addition placed in the candidate
  holding it; a removal no candidate holds left behind.

### 4. Samples And Documentation

- [ ] Give the web sample a YBN generation split into two files (see Decisions).
- [x] Describe the files and candidates in `docs/reference.md`, `docs/usage.md` and
  `docs/ARCHITECTURE.md`; replace the `docs/TODO.md` item on renumbered YBNs.

## Decisions

- YBN families keep no successors: the most covering candidate stands in where one file is
  named, as the merge report's `superseded_by` and for diffing sources against YBNs first
  shipped after the build.
- An archive standing for nothing (`not_declared`, a title update's patch of a pack archive
  the pack does not declare) gives its YBNs no candidates: 708 of the 739 YBNs without one on
  the local vanilla archive. Their changes are not carried; they are merged and deployed as
  they are changed. The other 31 are interiors and tunnels no loaded YBN of the replacing
  archives overlaps.
- On the local vanilla archive, 2575 of the 3314 YBNs the game does not load have candidates;
  2497 are covered whole by their first. Of the 2586 same-name successors, 2373 are the first
  candidate and 2412 a candidate.
- The web sample has no archives a change set invalidates, so a YBN the game does not load
  there is one first shipped after the build, whose candidates are any loaded YBN; a split
  generation would need the sample's activity, and is tested on its own instead.
