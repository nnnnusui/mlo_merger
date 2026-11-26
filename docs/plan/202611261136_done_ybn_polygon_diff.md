# YBN Polygon Diff Plan

## Goal

Give YBNs diff documents and overwrites as YMAPs have, comparing collision as the polygons of
the whole file grouped by child collision flags, as occlude model triangles are compared, and
merge them from those documents: removals and additions of every source, with added polygons
placed so connected ones share a geometry child.

## Findings

- The YBN merge in use (`merge_ybn_deltas`) reads the source files directly, without diff
  documents or overwrites. It already matches polygons across the whole file by world-space
  geometry within `ybn_position`: removals of every source apply, a removal of a geometry
  wins over another source's material or corner-flag change of it, and each source's added
  polygons are appended as subsets of the source child holding them.
- `YbnDiff` (`diff/semantic.rs`) compares polygons per child path and is unused.
- Over 3000 vanilla YBNs sampled at random:
  - Every root is a Composite, every child a GeometryBVH with an identity transform.
  - Children carry collision filter flags (`flags`, two `u32`); a file has 1 to 5 or more
    pairs. The most common pairs are `0x3e/0x07f3bec0`, `0x2/0x00610000` and
    `0x3c/0x07f3bec0`.
  - Geometries hold triangles (20.6 M) and boxes, capsules, cylinders and spheres (0.9 M).
  - A geometry holds up to 13539 vertices and 4855 polygons.
  - 5933 triangles repeat exactly in another child of their file, all in a child of other
    flags: one surface in several collision layers. Rounded to 0.1, 43778 more repeat in a
    child of the same flags.
- The child flags are CodeWalker's `EBoundCompositeFlags` pair: the first names what the
  collision is (`MAP_WEAPON`, `MAP_DYNAMIC`, `MAP_ANIMAL`, `MAP_COVER`, `MAP_VEHICLE`,
  `FOLIAGE`, `MAP_STAIRS`, ...), the second what it collides with (`PED`, `RAGDOLL`,
  `PROJECTILE`, `TEST_WEAPON`, `TEST_CAMERA`, ...). Of the common pairs, `0x3e/0x07f3bec0` is
  a map surface everything collides with, `0x3c/0x07f3bec0` the same without `MAP_WEAPON`,
  and `0x2/0x00610000` one only projectiles and weapon and camera tests meet. A polygon
  repeated under other flags is the same surface in another collision layer, so removing it
  from one layer keeps the others.

## Design

- **Groups.** A polygon belongs to the group of its child's flags. Groups play the part of
  occlude model flags: diffs, merging and placement keep them apart. Within a group, polygons
  the same within the tolerance count once.
- **Identity.** A polygon is its kind, material but for its colour, corner flags, radius and
  world-space positions: triangles in winding order from any corner, boxes by their eight
  corners, capsules and cylinders by their endpoints in either order. Polygons match one
  another within `ybn_position` per axis, as `merge_ybn_deltas` matched them; vertices are not
  merged, so close vertices of one file stay apart.
- **Diff.** Per group whose polygons change, the polygons removed and added, each resolved
  by value (positions, material, corner flags, radius, vertex colours).
- **Apply.** Per group: remove each removed polygon from the child holding it, warning about
  and skipping one the file lacks, and dropping children left empty; place each connected
  piece of the added polygons (by shared vertices) into the child of the group sharing most
  of its vertices that stays within `[ybn] max_geometry_vertices` (default 65534) and
  `max_geometry_polygons` (default 32767) and 256 materials, and fill new children with the rest, a
  piece over the limit split breadth-first. Changed children are rebuilt (vertices,
  materials, BVH); the others are kept as they are. The root bounds cover the children.
- **Merge.** Every source's removals apply. Additions of every source apply once each in
  source order, but those whose geometry another source removes, so a removal wins over
  material or corner-flag changes, as now.
- **Documents.** The source cache writes `diff/{resource}/{stream_path}.diff.json` for YBNs
  as for YMAPs; overwrites skip within a group named by its flags, as for occlude model
  triangles. YBN generations ([plan](202611191801_done_ybn_generations.md)) build on these
  documents.

## Phases

### 0. Baseline And Open Measurements

- [x] Record, outside the repository, the current merge's output for the local sources: per
  merged YBN, the polygons of each group with their child, and the children's bytes.
- [x] Measure whether children of one file and flags differ in other fields (margin,
  `unknown_9c`, `unknown_ac`, materials' unknown bits), which would join the group key.

### 1. Identity And Groups

- [x] Move polygon identity and lookup out of `legacy.rs` into a module for the diff, with
  group keys and a vertex index shared by the files compared.
- [x] Tests: winding, box corners, endpoint order, tolerance across a cell boundary, repeats
  counted once within a group and kept apart across groups.

### 2. Diff

- [x] Diff two YBNs into per-group removed and added polygons, and remove `YbnDiff`.
- [x] Tests: regrouped children and reordered polygons diff to nothing; a material change is
  a removal and an addition.

### 3. Apply

- [x] Apply as the design describes, rebuilding changed children and the root bounds.
- [x] Add `[ybn] max_geometry_vertices` (1 to 65534) and `max_geometry_polygons` (1 to 32767)
  to the config.
- [x] Tests: removal drops an emptied child; connected additions join their child; unlinked
  ones make a new child; the size limit splits a piece; untouched children keep their bytes.

### 4. Documents And Merge

- [x] Write YBN diff documents in the source cache, with their TypeScript types.
- [x] Merge YBNs from the documents with the policy above. A YBN one source replaces merges only
  when an overwrite skips within it, and is otherwise deployed as the source has it.
- [x] Skip within YBN diff documents in overwrites, and drop the `UnsupportedFormat` reason
  for YBN overwrites.

### 5. Checking Against The Baseline

Results on 311 local YBNs: a single source's merged file holds the source's polygons but for
524 of 2.0 M, near the tolerance; the earlier merge differed by 26665, dropping changes of child
flags. The whole merge takes 15 s against the earlier 13.5 s.


- [x] Rerun on the local sources and compare with phase 0: the polygons of each group match,
  but for repeats the new placement collapses; children no source changes keep their bytes.
- [x] Time diff, merge and apply against the current merge.

### 6. Samples And Docs

- [x] Regenerate the web sample, whose resources change `smpl_block_01.ybn`, with an overwrite
  leaving out the planter's collision.
- [x] Update `docs/reference.md` (YBN merge, diff and overwrite documents), `docs/usage.md`
  and `docs/TODO.md`.

## Decisions

- Exact repeats count once within a group; across groups they are different collision layers
  and stay. Polygons close within the tolerance in one file stay apart: vanilla has many
  triangles a few centimetres across.
- Material colours are left out of the identity: tools drop colour tables while keeping the
  indices into them, and the colours have no bearing on collision.
- A child holds at most 32767 polygons, which the GeometryBVH indexes with 16-bit signed
  indices, beside the 65534 vertices its 16-bit vertex indices allow.
- Source children other than geometries (primitive Bounds) are compared whole and added
  beside the others, as now.
