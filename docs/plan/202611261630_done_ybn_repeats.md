# YBN Repeats Plan

## Goal

Lighten the collision the game handles by leaving out polygons a YBN holds twice: in every YBN
the merge builds, and in the loaded vanilla YBNs where repeats are common enough to deploy them
for that alone.

## Findings

- Of the 72.7 million triangles of the 10612 loaded vanilla YBNs on the local vanilla archive,
  72605 (0.10%) repeat another of the same collision flags exactly: positions, winding, corner
  flags and material. 903 YBNs hold some; in 151 they are 1% or more of the triangles, in 52
  5% or more (42706 repeats), in 31 10% or more. Repeats stay within one geometry child.
- Counting them over every loaded vanilla YBN takes 3.2 seconds.
- A YBN diff already counts exact repeats in a flags group once, and removes them together;
  applying a diff keeps the repeats of the polygons it does not remove.

## Design

- **Repeats.** A polygon repeats another when it is the same polygon (the identity a YBN diff
  matches by, exactly) of the same collision flags in the same YBN. Leaving repeats out keeps
  the first and rebuilds the children it drops polygons from.
- **Merged YBNs.** Every YBN the merge builds from a diff leaves its repeats out.
- **Vanilla cache.** `ybn_repeats.json` lists the loaded YBNs holding repeats, each with its
  SHA-256, polygon count and repeat count.
- **Vanilla YBNs.** The merge also builds, from no source, each loaded vanilla YBN whose
  repeats are at least `vanilla_min_share` of its polygons, and a YBN only one resource
  replaces when its vanilla file is one of those.
- **Config.** `[ybn_repeats]` with `enabled` (default `false`) and `vanilla_min_share`
  (default `0.05`); the shipped config file enables it.
- **Report.** The merge report gives, per YBN, the repeats it left out.

## Phases

### 1. Leaving Repeats Out

- [x] Count and leave out a YBN's repeats in `core::format::ybn::diff::polygons`, and leave them
  out when applying a diff, as configured.
- [x] Tests: a repeat in a changed child and one in an unchanged child are left out; repeats of
  other flags stay.

### 2. Vanilla Cache

- [x] Write `ybn_repeats.json` (`YbnRepeatIndex`) for the loaded YBNs; bump the layout.

### 3. Merge

- [x] Read `[ybn_repeats]`; build the vanilla YBNs over the share and the single-source YBNs
  replacing them; invalidate merged YBNs when the config changes.
- [x] Report the repeats left out per YBN.
- [x] Tests on the web sample: a vanilla YBN with repeats is deployed without them.

### 4. Documentation

- [x] Describe repeats in `docs/reference.md`, `docs/usage.md`, `docs/ARCHITECTURE.md` and the
  config file.

## Decisions

- Repeats are the YBN diff's identity, exactly, so boxes, spheres, capsules and cylinders
  repeat too, and material colours are left out: the vanilla cache counts 82574 repeats in
  1802 loaded YBNs, 58 of them at 5% or more.
- The counts are rebuilt with the vanilla cache rather than kept per SHA-256: counting them
  all takes seconds.
- A YBN only one resource replaces is built from its diff, not deployed as it is, whenever
  repeats are left out of it, as its file holds vanilla's repeats.
- The merge report lists repeats in a file of its own, `report/ybn_repeats.json`, as the
  report index lists vanilla YMAPs.
