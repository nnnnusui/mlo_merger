# Entity Parents Across Generations Plan

## Goal

Carry the entities a resource adds with a parent, and the parent changes it makes, to the
generation of a YMAP the game loads, pointing them at the parent's counterpart there, or
making them orphans when it has none.

## Findings

- Merges carry entity additions without a parent only: one with a parent is listed in
  `generations/left_behind.json` as `addition_not_carried`. Parent changes are left out of
  the modifications carried.
- An entity's parent is an entity of the YMAP its YMAP's `parent` names, such as a `_lod`
  YMAP, which DLCs ship again under the same prefix as the YMAP itself.
- A merge already detaches a child whose parent it removes: the child points at no parent,
  and a high-detail one becomes `LODTYPES_DEPTH_ORPHANHD`.

## Rules

For an entity carried from a YMAP to its successor, with its parent in the older
generation's parent YMAP:

| Parent's counterpart in the loaded generation | Result |
|---|---|
| In the YMAP the successor's `parent` names | The entity points at the counterpart |
| Elsewhere, or none | The entity is detached: no parent, `LODTYPES_DEPTH_ORPHANHD` when high-detail |

## Phases

### 1. Measure

- [ ] Count the additions with a parent and the parent changes left behind in a merge of the
  sources, and how many of their parents pair with an entity of the loaded generation's
  parent YMAP.
- [ ] Check that `generations.json` pairs the parent YMAPs (`*_lod`, `*_slod`) of the
  families carried, and what the successor's `parent` names for each.

### 2. Carry Additions With A Parent

- [ ] Find the parent of each carried addition from its source's parent YMAP and index, and
  its counterpart through the units of the parent YMAP's family.
- [ ] Write the carried addition with its parent pointing at the counterpart, as the diff of
  an addition names a parent, or detached by the rules above.

### 3. Carry Parent Changes

- [ ] Carry a modification's parent change by the same lookup, detaching the entity when the
  new parent has no counterpart.

### 4. Report

- [ ] Report each entity carried detached, with the parent it lost, in the merge report and
  `left_behind.json`'s place for changes carried only in part.

### 5. Documentation

- [ ] Describe carrying parents in `docs/reference.md` and `docs/ARCHITECTURE.md`, and remove
  the item from `docs/TODO.md`.

## Decisions

- An entity whose parent has no counterpart is carried detached, not split into a new YMAP
  of its own: few entities would gain from it.
