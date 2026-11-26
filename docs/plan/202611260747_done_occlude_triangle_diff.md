# Occlude Triangle Diff Plan

## Goal

Compare, merge and carry occlude model changes as triangles of the whole YMAP, grouped by model
flags, rather than per model, so a source regrouping triangles into other models changes only
the triangles it moves in space. Merged triangles are placed into models so connected triangles
share a model where they fit.

## Findings

- The game culls occluder triangles on one side: a triangle occludes only when its normal
  `(b - a) x (c - a)` points at the camera. Two walls of opposite winding checked in game each
  occlude from one side only.
- Over the 227 vanilla YMAPs with occlude models (365001 triangles, none degenerate):
  - No triangle appears in two models of one file, compared exactly, with corners rotated,
    within 0.01 or 0.05 per axis, or with winding ignored.
  - 56 triangles repeat exactly within their model (13 files). 2199 have a twin of reversed
    winding in their model (82 files): occluders seen from both sides.
  - 64 models have `flags` 1, the others 0.
- Of the 29 pairs of successive vanilla versions of an occl YMAP whose triangles differ, 4 move
  kept triangles to other models, with the model count changing (9 to 8, 7 to 6). Pairing
  models by their bounds turns such a change into replacing whole models.
- Vanilla models do not follow connectivity: 609 of 2036 models are one connected piece and
  595 have ten or more, 211 files share vertices between models, models span 226 m in XY at the
  median, and 39% of model pairs of a file overlap in XY. Placement is free to group connected
  triangles.

## Design

- **Identity.** A triangle is its three corners in winding order, rotated to start at the
  least corner. Corners of one file within `ymap_occlude_model_position` per axis are one
  vertex: a spatial hash of vertices snaps each corner to the vertex found within the
  tolerance in its own and neighbouring cells, so keys compare exactly.
- **Diff.** `occlude_models` diffs as one entry per `flags` value whose triangles change, with
  the triangles removed and added under it. Exact repeats count once. Bounds are derived and
  not diffed; a model's flags change moves its triangles between groups.
- **Apply.** Per flags group:
  - Remove each removed triangle from the model holding it; drop models left empty.
  - Skip additions already present. Split the rest into connected pieces by shared vertices
    (union-find), counting vertices of the file's models as links.
  - Add each piece to the linked model sharing most of its vertices that stays within 255
    distinct vertices; otherwise pack pieces, in their order, into new models appended after
    the others. A piece over 255 vertices splits in breadth-first order so each part stays
    connected.
  - Give every changed model the exact bounds of its vertices; untouched models keep theirs.
  - Fit as [`YmapOccludeModel::fit`] does, so the merge planner's separate fit goes.
- **Merge.** Per flags group, every source's removals apply, and an addition equal to a
  removed vanilla triangle is dropped, as removal wins elsewhere; additions of every source
  apply once each in source order. Triangles take part in no item competition, so the
  occlude-model entries of merge reports and the `combined` reason go.

## Phases

### 0. Baseline From Local Sources

- [x] With the current build, regenerate the source-cache diffs of the local resources and
  record, outside the repository, per source YMAP with occlude models: the vanilla triangles
  each diff removes (from removed, replaced and modified models), the triangles it adds, and the
  occlude models of each merged YMAP.

### 1. Triangle Identity

- [x] Add the vertex snapping index and canonical triangle keys for a set of models.
- [x] Tests: rotated corners match, reversed winding differs, corners within the tolerance
  match across a cell boundary, exact repeats collapse.

### 2. Diff and Apply

- [x] Replace the per-model diff with a `#[diff(with)]` strategy on `Ymap::occlude_models`
  producing the flags groups, and drop the model pairing (`YmapOccludeModel::matches`) and
  its field diff.
- [x] Apply as the design describes, with connected placement and bounds.
- [x] Rename `ymap_occlude_model_position` to `ymap_occlude_vertex_position`, described as the
  tolerance snapping triangle corners, in the config and its docs.
- [x] Tests: regrouped vanilla triangles diff to nothing; removal empties and drops a model;
  a connected addition joins its model; an unlinked one makes a new model; a piece over the
  limit splits connected; a flags change moves triangles; the diff of a source applied to
  vanilla holds the source's triangles.

### 3. Merge, Reports and Overwrites

- [x] Merge the flags groups of every source; remove `merge_occlude_models` and the
  occlude-model handling of duplicate collection and reports, with `IgnoredReason::Combined`.
- [x] Thin triangles of an added YMAP several resources ship by key.
- [x] Update overwrite skip paths and their tests to the new diff layout.
- [x] Tests: removals of one source and additions of another both apply; equal additions of
  two sources apply once; re-adding a removed triangle is dropped.

### 4. Carrying to Generations

- [x] Carry triangle removals and additions to the successor generation by key, without
  pairing models: a removal missing there is listed in `left_behind.json`.
- [x] Drop `ItemKind::OccludeModels` from generation units.
- [x] Remove the occlude-triangle item from `docs/TODO.md`.

### 5. Checking Against the Baseline

- [x] Rerun on the same sources and check, per source YMAP, that the new diff's removed and added
  triangles are within those of the baseline: a vanilla triangle the old diff kept stays kept.
  List what the new diff drops for review, by cause: regrouped models, bounds-only changes,
  corners within the tolerance.
- [x] Check that each merged YMAP keeps every occlude model no source changes byte for byte,
  and every vanilla triangle no source removes.

### 6. Samples and Docs

- [x] Regenerate the web and overwrite samples; the web sample's second wall strip, linked to
  no vertex left after the roof's removal, makes its own model.
- [x] Update `docs/reference.md` (combining sources, merge report reasons, diff documents) and
  the TypeScript types.
