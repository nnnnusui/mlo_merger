# Occlude Model Fit Plan

## Goal

Deploy merged occlude models whose bounds hold all their triangles and whose triangles index
at most 255 distinct vertices, so combining the triangle changes of several sources neither
leaves triangles outside the bounds nor fails to encode, and write occlude models with the
padding values vanilla files carry.

## Findings

- Occlude model triangles carry no derived structure: the binary holds the distinct vertices
  and one index byte per corner, and nothing references triangles by position. Triangle order
  needs no rebuilding, unlike GeometryBVH polygons in YBNs.
- CodeWalker rebuilds every occlude model when saving: vertices deduplicated in triangle order,
  `bmin` / `bmax` set to the exact vertex bounds, and `Unused0` / `Unused1` (the 4 bytes after
  `bmin` and after `bmax`) set to the minimum and maximum X. Past 256 distinct vertices, it maps
  further vertices to index 0. Adding or removing a triangle keeps the order of the others.
- Over the 1861 vanilla occlude models with triangles:
  - Bounds hold every vertex to within 5e-7: 1671 models match the vertex bounds exactly, 187
    differ by float rounding, and 3 are up to 12.6 larger.
  - The largest model has 255 distinct vertices (6 models), 289 models have more than 200,
    and none has 256.
  - In all 1858 models found in the vanilla binaries, the float after `bmin` equals `bmin.x`
    and the float after `bmax` equals `bmax.x`. The encoder writes zero there.
- The merge combines the triangle changes of sources modifying one vanilla model, but takes
  `bmin` / `bmax` from the first source changing them, so triangles a later source adds can
  fall outside them. Combined additions can exceed the vertex limit, which the encoder rejects
  only past 256.

## Phases

### 1. Fitting a Model

- [x] Add a fit to `YmapOccludeModel` returning the model unchanged when its bounds hold every
  vertex within a small tolerance and it has at most 255 distinct vertices, and otherwise
  the models to deploy in its place:
  - Split the triangles in order into consecutive runs, each ending before the triangle that
    would bring its distinct vertices past 255.
  - Give each part the exact bounds of its vertices and the original's `flags`.
- [x] Lower the encoder's vertex limit to 255.
- [x] Unit tests: untouched in-bounds model, rounding-level excursion kept, triangle outside
  the bounds refits them, a model over the limit splits in order with each part in bounds.

### 2. Merge: Fitting After Apply

- [x] Fit every occlude model of the applied merged YMAP in the merge planner, in place, so a
  split model's parts follow one another where it was. Log each refit at `debug`, and each
  split at `info` with the YMAP name and part count.
- [x] Merge test: two sources adding triangles beyond the model's bounds on opposite sides;
  two sources whose additions together pass 255 vertices.
- [x] Describe the fit in the occlude model notes of the merge module docs.

### 3. Encoder: Bounds Padding

- [x] Write `bmin.x` into the 4 bytes after `bmin` and `bmax.x` into the 4 bytes after `bmax`
  of each `OccludeModel`, through a vector write filling the whole 16-byte `Float_XYZ` slot.
- [x] Test: an encoded occlude model carries both values; a vanilla occlusion YMAP
  round-trips byte for byte in those fields.
